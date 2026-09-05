use crate::agent::{Message, Preferences};
use serde::Serialize;
use std::{
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

pub fn start(
    tx: SyncSender<Message>,
    settings: Arc<RwLock<Preferences>>,
    health: Arc<RwLock<CaptureHealth>>,
) {
    std::thread::spawn(move || {
        let Ok(automation) = UIAutomation::new() else {
            report(
                &health,
                "",
                "Windows accessibility is unavailable; restart Lossy",
            );
            return;
        };
        let mut last = String::new();
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
            let (context, text) = match read_editor(&automation, pid, &exe, &title) {
                Ok(value) => value,
                Err(reason) => {
                    last.clear();
                    report(&health, &exe, reason);
                    continue;
                }
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
                    source: format!("{} · {}", exe.trim_end_matches(".exe"), title),
                    trusted: false,
                    committed,
                })
                .is_err()
            {
                report(&health, &exe, "Background writer stopped; restart Lossy");
                break;
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
    });
}

fn read_editor(
    automation: &UIAutomation,
    pid: u32,
    exe: &str,
    title: &str,
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
        || element.get_control_type().ok() != Some(ControlType::Edit)
    {
        return Err("This field does not expose a supported editable text control");
    }
    let text = if let Ok(pattern) = element.get_pattern::<UIValuePattern>() {
        if pattern.is_readonly().unwrap_or(true) {
            return Err("Read-only field excluded");
        }
        pattern
            .get_value()
            .map_err(|_| "Editor text is unavailable")?
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
    if text.len() > 1024 * 1024 {
        return Err("Editor exceeds the capture size limit");
    }
    let runtime = element.get_runtime_id().unwrap_or_default();
    if runtime.is_empty() {
        return Err("Editor has no stable accessibility identity");
    }
    let context = format!("native|{pid}|{exe}|{title}|{runtime:?}");
    if crate::platform::foreground() != Some((pid, exe.into(), title.into()))
        || !element.has_keyboard_focus().unwrap_or(false)
    {
        return Err("Focus changed; waiting for the next stable field");
    }
    if element.is_password().ok() != Some(false)
        || sensitive(&element.get_name().unwrap_or_default())
    {
        return Err("Field became protected while reading; snapshot discarded");
    }
    Ok((context, text))
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
