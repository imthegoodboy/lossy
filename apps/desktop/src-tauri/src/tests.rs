use crate::agent::{Preferences, Service};
use serde_json::{Value, json};
use std::sync::{Arc, RwLock};

fn service(path: &std::path::Path) -> Service {
    let mut service =
        Service::open(path.into(), Arc::new(RwLock::new(Preferences::default()))).unwrap();
    let prefs = Preferences {
        enabled: true,
        ..Preferences::default()
    };
    service
        .request(json!({"op":"settings","prefs":prefs}))
        .unwrap();
    service
}
fn items(s: &mut Service) -> Vec<Value> {
    s.request(json!({"op":"list"})).unwrap()["items"]
        .as_array()
        .unwrap()
        .clone()
}
fn capture(s: &mut Service, key: &str, text: &str, trusted: bool) {
    s.capture(key.into(), text.into(), "Synthetic editor".into(), trusted)
        .unwrap();
}

#[test]
fn broader_capture_is_opt_in_and_never_overrides_protected_apps() {
    let mut prefs: Preferences =
        serde_json::from_value(json!({"enabled":true,"allowed_apps":["notepad.exe"]})).unwrap();
    assert!(!prefs.all_desktop_apps);
    assert!(!crate::capture::allowed("orca.exe", &prefs));
    assert!(crate::capture::allowed("NOTEPAD.EXE", &prefs));
    prefs.all_desktop_apps = true;
    assert!(crate::capture::allowed("orca.exe", &prefs));
    assert!(crate::capture::allowed("new-editor.exe", &prefs));
    for exe in [
        "Chrome.exe",
        "MSEDGE.EXE",
        "Bitwarden.exe",
        "Lossy.exe",
        "WindowsTerminal.exe",
        "pwsh.exe",
        "WhatsApp.exe",
        "msedgewebview2.exe",
    ] {
        prefs.allowed_apps.push(exe.into());
        assert!(!crate::capture::allowed(exe, &prefs), "must exclude {exe}");
    }
}

#[test]
fn capture_setup_survives_restart_and_reports_metadata_only() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = service(dir.path());
    let mut prefs = s.prefs.clone();
    prefs.all_desktop_apps = true;
    s.request(json!({"op":"settings","prefs":prefs})).unwrap();
    prefs.allowed_apps = vec!["C:\\apps\\editor.exe".into()];
    assert!(s.request(json!({"op":"settings","prefs":prefs})).is_err());
    drop(s);
    let mut s = Service::open(dir.path().into(), Default::default()).unwrap();
    let status = s.request(json!({"op":"status"})).unwrap();
    assert_eq!(status["prefs"]["all_desktop_apps"], true);
    let native = status["native"].as_object().unwrap();
    assert_eq!(native.len(), 3);
    assert!(
        native.contains_key("state")
            && native.contains_key("app")
            && native.contains_key("checked_at")
    );
}

#[test]
fn process_ownership_fails_closed_for_unknown_processes() {
    let pid = std::process::id();
    assert!(crate::platform::same_application_process(pid, pid));
    assert!(!crate::platform::same_application_process(pid, 0));
    assert!(!crate::platform::same_application_process(pid, u32::MAX));
}

#[test]
fn arrangement_survives_restart_pagination_and_new_capture() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = service(dir.path());
    let mut ids = Vec::new();
    for i in 0..65 {
        let note = s.request(json!({"op":"save","heading":format!("Card {i}"),"text":"Synthetic arrangement test"})).unwrap();
        ids.push(note["id"].clone());
    }
    s.request(json!({"op":"reorder","ids":ids})).unwrap();
    assert_eq!(items(&mut s)[0]["id"], ids[0]);
    let last = s.request(json!({"op":"list","offset":60})).unwrap();
    assert_eq!(last["items"][0]["id"], ids[60]);
    // A visible-page reorder must preserve the unseen tail.
    let mut first = ids[..60].to_vec();
    first.swap(0, 3);
    s.request(json!({"op":"reorder","ids":first})).unwrap();
    assert!(
        s.request(json!({"op":"reorder","ids":[ids[0],ids[0]]}))
            .is_err()
    );
    assert!(
        s.request(json!({"op":"reorder","ids":["not-an-id"]}))
            .is_err()
    );
    drop(s);
    let mut s = service(dir.path());
    assert_eq!(items(&mut s)[0]["id"], ids[3]);
    capture(&mut s, "arrangement/new", "New synthetic draft", true);
    assert_eq!(items(&mut s)[0]["id"], ids[3]);
    let last = s.request(json!({"op":"list","offset":60})).unwrap();
    assert_eq!(last["items"][0]["id"], ids[60]);
    assert_eq!(last["items"][5]["text"], "New synthetic draft");
}

#[test]
fn conversations_resume_independently_including_after_agent_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = service(dir.path());
    capture(&mut s, "tab/chat-a/editor", "Hello A", true);
    capture(&mut s, "tab/chat-b/editor", "Hello B", true);
    capture(&mut s, "tab/chat-a/editor", "Hello A continued", true);
    let before = items(&mut s);
    assert_eq!(before.len(), 2);
    let a = before
        .iter()
        .find(|x| x["text"] == "Hello A continued")
        .unwrap()
        .clone();
    drop(s);
    let mut s = service(dir.path());
    capture(&mut s, "tab/chat-a/editor", "Hello A resumed", true);
    assert_eq!(items(&mut s).len(), 2);
    let restored = s.request(json!({"op":"get","id":a["id"]})).unwrap();
    assert_eq!(restored["text"], "Hello A resumed");
    assert_eq!(restored["revision"], 3);
    capture(&mut s, "tab/chat-a/editor", "", true);
    capture(&mut s, "tab/chat-a/editor", "Next message", true);
    assert_eq!(items(&mut s).len(), 3);
}

#[test]
fn uncertain_native_replacements_split_and_deletes_do_not_immediately_reappear() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = service(dir.path());
    capture(&mut s, "native", "First draft", false);
    capture(&mut s, "native", "First draft continued", false);
    assert_eq!(items(&mut s).len(), 1);
    capture(&mut s, "native", "Unrelated draft", false);
    let all = items(&mut s);
    assert_eq!(all.len(), 2);
    let current = all.iter().find(|x| x["text"] == "Unrelated draft").unwrap();
    s.request(json!({"op":"delete","id":current["id"],"revision":current["revision"]}))
        .unwrap();
    capture(&mut s, "native", "Unrelated draft", false);
    assert_eq!(items(&mut s).len(), 1);
    capture(&mut s, "native", "Unrelated draft new", false);
    assert_eq!(items(&mut s).len(), 2);
}

#[test]
fn pause_private_secure_and_disabled_capture_are_not_saved() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = service(dir.path());
    for flags in [
        json!({"private":true,"secure":false}),
        json!({"private":false,"secure":true}),
        json!({}),
    ] {
        let mut req = flags;
        req["op"] = json!("browser_capture");
        req["text"] = json!("Never persist this synthetic secret");
        req["context"] = json!("private");
        s.request(req).unwrap();
    }
    s.prefs.paused = true;
    capture(&mut s, "a", "Paused synthetic content", true);
    s.prefs.paused = false;
    s.prefs.enabled = false;
    capture(&mut s, "a", "Disabled synthetic content", true);
    assert!(items(&mut s).is_empty());
    assert!(crate::capture::sensitive("Password"));
    assert!(!crate::capture::allowed("chrome.exe", &s.prefs));
}

#[test]
fn command_validation_recovery_copies_filters_and_backup() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = service(dir.path());
    capture(&mut s, "chat", "नमस्ते 🌸", true);
    let captured = items(&mut s)[0].clone();
    assert!(
        s.request(json!({"op":"save","id":captured["id"],"revision":1,"text":"overwrite"}))
            .is_err()
    );
    let note = s
        .request(json!({"op":"save","heading":"Recovery","text":"नमस्ते 🌸 continued"}))
        .unwrap();
    s.request(json!({"op":"pin","id":note["id"],"pinned":true}))
        .unwrap();
    let pinned = s
        .request(json!({"op":"list","filter":"pinned","search":"continued"}))
        .unwrap();
    assert_eq!(pinned["items"].as_array().unwrap().len(), 1);
    assert!(s.request(json!({"op":"get","id":"bad"})).is_err());
    assert!(s.request(json!({"op":"unknown"})).is_err());
    s.request(json!({"op":"backup"})).unwrap();
    let path = std::fs::read_dir(dir.path().join("backups"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let backup = lossy_storage::Store::open(path).unwrap();
    assert_eq!(backup.list(10, 0).unwrap().len(), 2);
}

#[test]
fn box_color_survives_restart_and_rejects_unknown_styles() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = service(dir.path());
    capture(&mut s, "color-test", "Synthetic colored box", true);
    let item = items(&mut s)[0].clone();
    let changed = s
        .request(json!({"op":"color","id":item["id"],"color":"sage"}))
        .unwrap();
    assert_eq!(changed["color"], "sage");
    assert!(
        s.request(json!({"op":"color","id":item["id"],"color":"url(unsafe)"}))
            .is_err()
    );
    drop(s);
    let mut reopened = service(dir.path());
    assert_eq!(items(&mut reopened)[0]["color"], "sage");
}
