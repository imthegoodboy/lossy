use crate::agent::{Preferences, Service, normalize_line_endings, related_draft, similar_draft};
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
type Prior = (String, String, Option<String>);
fn prior_view(prior: &Prior) -> (&str, &str, Option<&str>) {
    (prior.0.as_str(), prior.1.as_str(), prior.2.as_deref())
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
    assert!(crate::capture::sensitive("API key"));
    assert!(crate::capture::sensitive("One-time verification code"));
    assert!(!crate::capture::sensitive("Search messages"));
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

/// Verify supported control types and that related edits share a card while replacements split.
#[test]
fn native_document_controls_and_mid_draft_edits_are_supported() {
    // Windows 11 Notepad exposes ControlType.Document, not the classic Edit type.
    assert!(crate::capture::supported_control(
        uiautomation::types::ControlType::Document
    ));
    assert!(crate::capture::supported_control(
        uiautomation::types::ControlType::Edit
    ));
    assert!(!crate::capture::supported_control(
        uiautomation::types::ControlType::Text
    ));
    assert!(related_draft(
        "The quick brown fox jumps over the lazy dog",
        "The quick brown cat jumps over the lazy dog"
    ));
    assert!(!related_draft(
        "First synthetic draft",
        "A completely separate note"
    ));

    let dir = tempfile::tempdir().unwrap();
    let mut s = service(dir.path());
    capture(&mut s, "native/mid-edit", "The quick brown fox", false);
    capture(&mut s, "native/mid-edit", "The quick red fox", false);
    let all = items(&mut s);
    assert_eq!(all.len(), 1);
    assert_eq!(all[0]["text"], "The quick red fox");
    capture(&mut s, "native/mid-edit", "Unrelated replacement", false);
    assert_eq!(items(&mut s).len(), 2);
}

/// Verify CRLF and bare CR normalization both directly and in stored captures.
#[test]
fn native_line_endings_are_normalized() {
    assert_eq!(normalize_line_endings("a\r\nb\rc\n"), "a\nb\nc\n");
    assert_eq!(
        normalize_line_endings("already\nnormalized"),
        "already\nnormalized"
    );

    let dir = tempfile::tempdir().unwrap();
    let mut s = service(dir.path());
    capture(
        &mut s,
        "native/line-endings",
        "first\r\nsecond\rthird",
        false,
    );
    assert_eq!(items(&mut s)[0]["text"], "first\nsecond\nthird");
}

/// Verify clipboard app exclusions, standalone card storage, and own-copy suppression.
#[test]
fn clipboard_policy_and_storage_are_separate_from_draft_scope() {
    for app in [
        "chrome.exe",
        "msedge.exe",
        "notepad.exe",
        "windowsterminal.exe",
    ] {
        assert!(
            crate::capture::clipboard_exclusion(app).is_none(),
            "{app} copies should be allowed"
        );
    }
    for app in [
        "lossy.exe",
        "bitwarden.exe",
        "1Password.exe",
        "KeePassXC.exe",
    ] {
        assert!(
            crate::capture::clipboard_exclusion(app).is_some(),
            "{app} copies should be excluded"
        );
    }

    let dir = tempfile::tempdir().unwrap();
    let mut s = service(dir.path());
    s.ignored_clipboard = 7;
    s.clipboard(
        "Synthetic clipboard text".into(),
        "clipboard".into(),
        "Clipboard · notepad".into(),
        8,
    )
    .unwrap();
    s.clipboard(
        "Duplicate of Lossy's own copy".into(),
        "clipboard".into(),
        "Clipboard · lossy".into(),
        7,
    )
    .unwrap();
    let all = items(&mut s);
    assert_eq!(all.len(), 1);
    assert_eq!(all[0]["kind"], "clipboard");
    assert_eq!(all[0]["text"], "Synthetic clipboard text");
}

/// Verify transient titles preserve context while unrelated documents select a new title.
#[test]
fn volatile_window_titles_do_not_split_native_drafts() {
    use crate::capture::{context_title, title_pending};

    assert_eq!(
        context_title("", "notepad.exe", None, "Lossy value check"),
        "notepad.exe"
    );
    let notepad_prior = (
        "notepad.exe".to_owned(),
        "Lossy value check abc".to_owned(),
        None,
    );
    // Windows 11 Notepad uses the first line as its title and briefly reports
    // an empty title while the edit is committed.
    assert_eq!(
        context_title(
            "",
            "notepad.exe",
            Some(prior_view(&notepad_prior)),
            "changed"
        ),
        "notepad.exe"
    );
    // An unrelated snapshot under a transient empty title waits for the new
    // title instead of contaminating the previous card and then duplicating it.
    assert!(title_pending(
        "",
        Some(prior_view(&notepad_prior)),
        "A different document"
    ));
    assert!(!title_pending(
        "",
        Some(prior_view(&notepad_prior)),
        "Lossy value check abc edited"
    ));
    assert!(!title_pending(
        "A different document",
        Some(prior_view(&notepad_prior)),
        "A different document"
    ));
    assert_eq!(
        context_title(
            "Lossy value check abc edited",
            "notepad.exe",
            Some(prior_view(&notepad_prior)),
            "Lossy value check abc edited"
        ),
        "notepad.exe"
    );
    // A genuine document switch has unrelated text and takes the new title.
    assert_eq!(
        context_title(
            "A different document",
            "notepad.exe",
            Some(prior_view(&notepad_prior)),
            "A different document"
        ),
        "A different document"
    );
    let same_title_prior = (
        "Lossy value check abc".to_owned(),
        "First synthetic sentence".to_owned(),
        Some("Lossy value check abc".to_owned()),
    );
    assert_eq!(
        context_title(
            "Lossy value check abc",
            "notepad.exe",
            Some(prior_view(&same_title_prior)),
            "A completely separate note"
        ),
        "Lossy value check abc"
    );
}

/// Verify Notepad title churn updates one stored card and increments its revision.
#[test]
fn template_documents_with_different_titles_stay_apart() {
    use crate::capture::context_title;

    let first = "Hi Alice,\nThanks,\nBob";
    let second = "Hi Alice,\nThanks,\nEve";
    assert!(!similar_draft(first, second));
    assert!(similar_draft(
        "The quick brown fox jumps over the lazy dog",
        "The quick brown cat jumps over the lazy dog"
    ));

    // The title changed because the first line was edited: same document.
    let first_line_prior = (
        "notepad.exe".to_owned(),
        "The quick brown fox jumps over the lazy dog".to_owned(),
        Some("The quick brown fox jumps over the lazy dog".to_owned()),
    );
    assert_eq!(
        context_title(
            "The quick brown cat jumps over the lazy dog",
            "notepad.exe",
            Some(prior_view(&first_line_prior)),
            "The quick brown cat jumps over the lazy dog"
        ),
        "notepad.exe"
    );
    // A different template under a new title takes its own context.
    let template_prior = (
        "Email draft".to_owned(),
        first.to_owned(),
        Some("Email draft".to_owned()),
    );
    assert_eq!(
        context_title(
            "Second email",
            "notepad.exe",
            Some(prior_view(&template_prior)),
            second
        ),
        "Second email"
    );
}

#[test]
fn notepad_title_churn_reuses_one_card_end_to_end() {
    use crate::capture::context_title;

    let initial = "Lossy value check abc\nsecond line";
    let edited = "Lossy value check abc edited";
    let pid = 4242;
    let exe = "notepad.exe";
    let runtime = "[42, 7]";

    // First poll: Windows briefly reports an empty title.
    let first_title = context_title("", exe, None, initial);
    let first_context = format!("native|{pid}|{exe}|{first_title}|{runtime}");
    let prior = (first_title.clone(), initial.to_owned(), None);
    // Next poll: Notepad replaces the title with the first line while the edit
    // continues. The context must stay on the app-name fallback.
    let next_title = context_title(
        "Lossy value check abc edited",
        exe,
        Some(prior_view(&prior)),
        edited,
    );
    let next_context = format!("native|{pid}|{exe}|{next_title}|{runtime}");
    assert_eq!(first_context, next_context);

    let dir = tempfile::tempdir().unwrap();
    let mut s = service(dir.path());
    capture(&mut s, &first_context, initial, false);
    capture(&mut s, &next_context, edited, false);
    let all = items(&mut s);
    assert_eq!(all.len(), 1);
    assert_eq!(all[0]["text"], edited);
    assert_eq!(all[0]["revision"], 2);
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
