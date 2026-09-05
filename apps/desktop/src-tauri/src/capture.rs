use crate::agent::{Message, Preferences};
use std::{
    sync::{Arc, RwLock, mpsc::SyncSender},
    time::Duration,
};
use uiautomation::{
    UIAutomation,
    patterns::{UITextPattern, UIValuePattern},
    types::ControlType,
};

pub fn start(tx: SyncSender<Message>, settings: Arc<RwLock<Preferences>>) {
    std::thread::spawn(move || {
        let Ok(automation) = UIAutomation::new() else {
            return;
        };
        let mut last = String::new();
        loop {
            std::thread::sleep(Duration::from_millis(35));
            let prefs = settings.read().unwrap().clone();
            if !prefs.enabled || prefs.paused || !crate::platform::desktop_available() {
                last.clear();
                continue;
            }
            let Some((pid, exe, title)) = crate::platform::foreground() else {
                continue;
            };
            if !allowed(&exe, &prefs) {
                last.clear();
                continue;
            }
            let Ok(element) = automation.get_focused_element() else {
                continue;
            };
            if element.get_process_id().ok() != Some(pid)
                || element.is_password().ok() != Some(false)
                || element.has_keyboard_focus().ok() != Some(true)
            {
                last.clear();
                continue;
            }
            if element.get_control_type().ok() != Some(ControlType::Edit) {
                last.clear();
                continue;
            }
            let field = element.get_name().unwrap_or_default();
            if sensitive(&field) {
                last.clear();
                continue;
            }
            let value = if let Ok(pattern) = element.get_pattern::<UIValuePattern>() {
                if pattern.is_readonly().unwrap_or(true) {
                    continue;
                }
                pattern.get_value().ok()
            } else {
                element
                    .get_pattern::<UITextPattern>()
                    .ok()
                    .and_then(|p| p.get_document_range().ok())
                    .and_then(|r| r.get_text(262144).ok())
            };
            let Some(text) = value else {
                continue;
            };
            let runtime = element.get_runtime_id().unwrap_or_default();
            if runtime.is_empty() {
                continue;
            }
            let context = format!("native|{pid}|{exe}|{title}|{runtime:?}");
            // Recheck ownership after the potentially slow accessibility call.
            if crate::platform::foreground() != Some((pid, exe.clone(), title.clone()))
                || !element.has_keyboard_focus().unwrap_or(false)
            {
                continue;
            }
            let fingerprint = blake3::hash(format!("{context}|{text}").as_bytes())
                .to_hex()
                .to_string();
            if fingerprint == last {
                continue;
            }
            last = fingerprint;
            let _ = tx.send(Message::Capture {
                context,
                text,
                source: format!("{} · {}", exe.trim_end_matches(".exe"), title),
                trusted: false,
            });
        }
    });
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
pub fn allowed(exe: &str, prefs: &Preferences) -> bool {
    ![
        "lossy.exe",
        "1password.exe",
        "bitwarden.exe",
        "keepass.exe",
        "keepassxc.exe",
        "chrome.exe",
        "msedge.exe",
        "firefox.exe",
        "brave.exe",
        "whatsapp.exe",
    ]
    .contains(&exe)
        && prefs
            .allowed_apps
            .iter()
            .any(|app| app.eq_ignore_ascii_case(exe))
}
