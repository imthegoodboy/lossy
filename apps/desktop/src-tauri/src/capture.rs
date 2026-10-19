use crate::agent::{Message, Preferences, related_draft};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{
        Arc, RwLock,
        mpsc::{self, SyncSender},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uiautomation::{
    UIAutomation,
    patterns::{UITextPattern, UIValuePattern},
    types::ControlType,
};

#[derive(Clone, Serialize)]
pub struct CaptureHealth {
    pub state: String,
    pub app: String,
    pub checked_at: u64,
}
impl Default for CaptureHealth {
    fn default() -> Self {
        Self {
            state: "Waiting for a supported editor".into(),
            app: String::new(),
            checked_at: 0,
        }
    }
}
pub fn report(health: &RwLock<CaptureHealth>, app: &str, state: &str) {
    let mut health = health.write().unwrap();
    health.app = app.into();
    health.state = state.into();
    health.checked_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
}

/// Spawn native editor polling with retrying accessibility initialization.
/// Preserve context across related edits and deduplicate acknowledged snapshots.
pub fn start(
    tx: SyncSender<Message>,
    settings: Arc<RwLock<Preferences>>,
    health: Arc<RwLock<CaptureHealth>>,
) {
    std::thread::spawn(move || {
        let mut retry_delay = Duration::from_secs(1);
        'agent: loop {
            // UI Automation can be briefly unavailable during sign-in or desktop
            // transitions. Retrying here keeps a transient failure from disabling
            // capture until the user restarts the whole app.
            let automation = match UIAutomation::new() {
                Ok(automation) => automation,
                Err(_) => {
                    report(
                        &health,
                        "",
                        "Windows accessibility is unavailable; retrying",
                    );
                    std::thread::sleep(retry_delay);
                    retry_delay = (retry_delay * 2).min(Duration::from_secs(30));
                    continue;
                }
            };
            let mut last = String::new();
            // Window titles are not always stable: Windows 11 Notepad uses the
            // first line as its title, and some editors briefly report an empty
            // title while redrawing. Remember the title chosen for each editor
            // control, keep it while its text continues the same draft, and only
            // switch when the text is clearly unrelated (a new document).
            let mut identities: HashMap<String, (String, String)> = HashMap::new();
            loop {
                std::thread::sleep(Duration::from_millis(35));
                let prefs = settings.read().unwrap().clone();
                let inactive = if !prefs.enabled {
                    Some("Saving is off")
                } else if prefs.paused {
                    Some("Saving is paused")
                } else if !crate::platform::desktop_available() {
                    Some("Locked or secure desktop excluded")
                } else {
                    None
                };
                if let Some(reason) = inactive {
                    last.clear();
                    report(&health, "", reason);
                    continue;
                }
                let Some((pid, exe, title)) = crate::platform::foreground() else {
                    report(&health, "", "No accessible foreground application");
                    continue;
                };
                // Keep the last external field's result visible while the archive is open.
                if exe == "lossy.exe" {
                    continue;
                }
                if let Some(reason) = exclusion(&exe) {
                    last.clear();
                    report(&health, &exe, reason);
                    continue;
                }
                if !allowed(&exe, &prefs) {
                    last.clear();
                    report(&health, &exe, "Application not enabled in Capture setup");
                    continue;
                }
                let (runtime, text) = match read_editor(&automation, pid, &exe) {
                    Ok(value) => value,
                    Err(reason) => {
                        last.clear();
                        report(&health, &exe, reason);
                        continue;
                    }
                };
                let stable = stable_title(&title);
                let identity = format!("{pid}|{exe}|{runtime}");
                let previous = identities.get(&identity);
                let context_title = context_title(&stable, &exe, previous, &text);
                // Do not replace a known editor's title with a transient empty
                // title. That would lose the document identity just as the user
                // switches documents. With no prior state, remember the empty
                // title as the app name so the first real title can refine it.
                if !stable.is_empty() || previous.is_none() {
                    if identities.len() >= 128 && !identities.contains_key(&identity) {
                        identities.clear();
                    }
                    identities.insert(identity, (context_title.clone(), text.clone()));
                }
                let context = format!("native|{pid}|{exe}|{context_title}|{runtime}");
                let display_title = if stable.is_empty() {
                    exe.trim_end_matches(".exe").to_owned()
                } else {
                    format!("{} · {stable}", exe.trim_end_matches(".exe"))
                };
                report(&health, &exe, "Supported editable field detected");
                let fingerprint = blake3::hash(format!("{context}|{text}").as_bytes())
                    .to_hex()
                    .to_string();
                if fingerprint == last {
                    continue;
                }
                let (committed, result) = mpsc::sync_channel(1);
                if tx
                    .send(Message::Capture {
                        context,
                        text,
                        source: display_title,
                        trusted: false,
                        committed,
                    })
                    .is_err()
                {
                    report(&health, &exe, "Background writer stopped; restart Lossy");
                    break 'agent;
                }
                if result
                    .recv_timeout(Duration::from_secs(10))
                    .unwrap_or(false)
                {
                    last = fingerprint;
                } else {
                    report(
                        &health,
                        &exe,
                        "Snapshot not committed; check the save error in Lossy",
                    );
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
        }
    });
}

/// Modern Notepad, Word and other document editors expose a writable
/// `ControlType::Document` rather than the classic `ControlType::Edit`.
/// Accept both; protected/read-only checks still run before reading text.
pub fn supported_control(control_type: ControlType) -> bool {
    matches!(control_type, ControlType::Edit | ControlType::Document)
}

/// Strip volatile dirty markers some editors put at the front of the window
/// title. Stable titles keep one draft in one card across saving/renaming.
fn stable_title(title: &str) -> String {
    let title = title.trim();
    title
        .strip_prefix('*')
        .unwrap_or(title)
        .trim_start()
        .to_owned()
}

/// Choose the title used in a native context key. Empty titles and title
/// changes caused by editing the same draft keep the original context; a
/// different title with unrelated text selects a new document context.
pub fn context_title(
    stable: &str,
    exe: &str,
    previous: Option<&(String, String)>,
    text: &str,
) -> String {
    match previous {
        Some((context_title, previous_text))
            if stable.is_empty()
                || stable == context_title
                || (!previous_text.is_empty() && related_draft(previous_text, text)) =>
        {
            context_title.clone()
        }
        _ => {
            if stable.is_empty() {
                exe.to_owned()
            } else {
                stable.to_owned()
            }
        }
    }
}

/// Read a focused Edit or Document control as its runtime identity and normalized text.
/// Reject unverifiable ownership, protected fields, oversized text, and focus changes.
fn read_editor(
    automation: &UIAutomation,
    pid: u32,
    exe: &str,
) -> Result<(String, String), &'static str> {
    let element = automation
        .get_focused_element()
        .map_err(|_| "Focused field is not accessible")?;
    if !element
        .get_process_id()
        .ok()
        .is_some_and(|owner| crate::platform::same_application_process(pid, owner))
    {
        return Err("Editor process ownership could not be verified");
    }
    if element.is_password().ok() != Some(false)
        || sensitive(&element.get_name().unwrap_or_default())
    {
        return Err("Protected or sensitive field excluded");
    }
    if element.has_keyboard_focus().ok() != Some(true)
        || !element
            .get_control_type()
            .ok()
            .is_some_and(supported_control)
    {
        return Err("This field does not expose a supported editable text control");
    }
    // Prefer ValuePattern when it is available and writable. Document controls
    // such as Notepad expose it; Word and some Electron editors only expose
    // TextPattern, so fall through instead of giving up on a ValuePattern error.
    let mut value = None;
    if let Ok(pattern) = element.get_pattern::<UIValuePattern>() {
        if pattern.is_readonly().unwrap_or(true) {
            return Err("Read-only field excluded");
        }
        if let Ok(text) = pattern.get_value() {
            value = Some(text);
        }
    }
    let text = if let Some(text) = value {
        if text.chars().count() > 262144 {
            return Err("Editor exceeds the capture size limit");
        }
        text
    } else {
        let range = element
            .get_pattern::<UITextPattern>()
            .ok()
            .and_then(|p| p.get_document_range().ok())
            .ok_or("Editor text is unavailable")?;
        // One extra character detects truncation instead of saving an incomplete snapshot.
        let text = range
            .get_text(262145)
            .map_err(|_| "Editor text is unavailable")?;
        if text.chars().count() > 262144 {
            return Err("Editor exceeds the capture size limit");
        }
        text
    };
    let text = crate::agent::normalize_line_endings(&text);
    if text.len() > 1024 * 1024 {
        return Err("Editor exceeds the capture size limit");
    }
    let runtime = element.get_runtime_id().unwrap_or_default();
    if runtime.is_empty() {
        return Err("Editor has no stable accessibility identity");
    }
    // Do not compare the window title here. Editing the first line of a
    // Notepad document changes the title while the same control keeps focus.
    // Ownership + keyboard focus are the stable checks.
    if !crate::platform::foreground().is_some_and(|(current_pid, current_exe, _)| {
        current_exe == exe && crate::platform::same_application_process(pid, current_pid)
    }) || !element.has_keyboard_focus().unwrap_or(false)
    {
        return Err("Focus changed; waiting for the next stable field");
    }
    if element.is_password().ok() != Some(false)
        || sensitive(&element.get_name().unwrap_or_default())
    {
        return Err("Field became protected while reading; snapshot discarded");
    }
    Ok((format!("{runtime:?}"), text))
}

pub fn sensitive(field: &str) -> bool {
    let name = field.to_lowercase();
    [
        "password",
        "passcode",
        "one-time",
        "verification code",
        "credit card",
        "card number",
        "cvv",
        "secret",
        "api key",
        "token",
    ]
    .iter()
    .any(|word| name.contains(word))
}

pub fn exclusion(exe: &str) -> Option<&'static str> {
    match exe.to_ascii_lowercase().as_str() {
        "lossy.exe" | "1password.exe" | "bitwarden.exe" | "keepass.exe" | "keepassxc.exe"
        | "dashlane.exe" | "nordpass.exe" => Some("Protected application excluded"),
        "chrome.exe" | "msedge.exe" | "msedgewebview2.exe" | "firefox.exe" | "brave.exe"
        | "opera.exe" | "vivaldi.exe" | "arc.exe" | "zen.exe" | "waterfox.exe" | "floorp.exe" => {
            Some("Browser excluded from desktop capture; use the Chrome / Edge companion")
        }
        "windowsterminal.exe"
        | "openconsole.exe"
        | "conhost.exe"
        | "cmd.exe"
        | "powershell.exe"
        | "pwsh.exe"
        | "wezterm-gui.exe"
        | "alacritty.exe"
        | "mintty.exe" => {
            Some("Terminal prompts are not supported; terminal history is never scraped")
        }
        "whatsapp.exe" => Some("Use WhatsApp Web with the companion for separate conversations"),
        _ => None,
    }
}
pub fn allowed(exe: &str, prefs: &Preferences) -> bool {
    exclusion(exe).is_none()
        && (prefs.all_desktop_apps
            || prefs
                .allowed_apps
                .iter()
                .any(|app| app.eq_ignore_ascii_case(exe)))
}

/// Clipboard copies are an explicit user action and live in their own history,
/// not in the draft-scope allowlist. Capture from any desktop app except apps
/// whose whole purpose is holding secrets, and Lossy's own copies.
pub fn clipboard_exclusion(exe: &str) -> Option<&'static str> {
    match exe.to_ascii_lowercase().as_str() {
        "lossy.exe" => Some("Lossy's own copies are skipped"),
        "1password.exe" | "bitwarden.exe" | "keepass.exe" | "keepassxc.exe" | "dashlane.exe"
        | "nordpass.exe" | "enpass.exe" | "protonpass.exe" | "keeper.exe" | "roboform.exe" => {
            Some("Password manager copies are excluded")
        }
        _ => None,
    }
}
