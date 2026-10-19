use base64::{Engine, engine::general_purpose::STANDARD};
use lossy_storage::{DraftContent, ItemId, SavedDraft, Store};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, RwLock,
        mpsc::{self, SyncSender},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub enabled: bool,
    pub paused: bool,
    pub clipboard: bool,
    pub autostart: bool,
    pub retention_days: u32,
    pub allowed_apps: Vec<String>,
    pub browser_capture: bool,
    pub all_desktop_apps: bool,
}
/// Apps a new install starts with. Existing installs keep their own list; the
/// setup panel can merge this list without replacing a deliberate selection.
pub fn recommended_apps() -> Vec<String> {
    [
        "notepad.exe",
        "wordpad.exe",
        "winword.exe",
        "excel.exe",
        "powerpnt.exe",
        "outlook.exe",
        "mspaint.exe",
        "snippingtool.exe",
        "explorer.exe",
        "code.exe",
        "cursor.exe",
        "windsurf.exe",
        "zed.exe",
        "vscodium.exe",
        "sublime_text.exe",
        "notepad++.exe",
        "idea64.exe",
        "pycharm64.exe",
        "webstorm64.exe",
        "obsidian.exe",
        "notion.exe",
        "slack.exe",
        "discord.exe",
        "telegram.exe",
        "signal.exe",
        "ms-teams.exe",
        "teams.exe",
        "orca.exe",
        "claude.exe",
    ]
    .map(str::to_owned)
    .to_vec()
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            enabled: false,
            paused: false,
            clipboard: true,
            autostart: false,
            retention_days: 30,
            allowed_apps: recommended_apps(),
            browser_capture: true,
            all_desktop_apps: false,
        }
    }
}

/// Accessibility text can arrive with CRLF or bare CR line endings. Normalize once
/// so previews render cleanly and the same draft is not split over line-ending changes.
pub fn normalize_line_endings(text: &str) -> String {
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text.to_owned()
    }
}

/// Native editors do not expose whether a wholesale replacement is the same draft.
/// Treat an edit as the same draft when the old and new values share a large
/// prefix/suffix, which covers mid-text edits and auto-formatting. Unrelated text
/// still starts a new card so separate messages are never merged by accident.
pub fn related_draft(before: &str, after: &str) -> bool {
    if before == after || before.starts_with(after) || after.starts_with(before) {
        return true;
    }
    let before_chars: Vec<char> = before.chars().collect();
    let after_chars: Vec<char> = after.chars().collect();
    let shortest = before_chars.len().min(after_chars.len());
    if shortest < 8 {
        return false;
    }
    let prefix = before_chars
        .iter()
        .zip(&after_chars)
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = before_chars[prefix..]
        .iter()
        .rev()
        .zip(after_chars[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    prefix + suffix >= shortest.div_ceil(2)
}

pub enum Message {
    Request(Value, SyncSender<Result<Value, String>>),
    Capture {
        context: String,
        text: String,
        source: String,
        trusted: bool,
        committed: SyncSender<bool>,
    },
    Clipboard {
        text: String,
        kind: String,
        source: String,
        sequence: u32,
    },
}
pub fn directory() -> PathBuf {
    // Test mode gets an isolated temporary folder; regular installs use per-user LocalAppData.
    std::env::var_os("LOSSY_TEST_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default()).join("Lossy")
        })
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
pub fn id_text(id: ItemId) -> String {
    id.0.iter().map(|b| format!("{b:02x}")).collect()
}
fn parse_id(text: &str) -> Result<ItemId, String> {
    if text.len() != 32 || !text.is_ascii() {
        return Err("Invalid item".into());
    }
    let mut bytes = [0; 16];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).map_err(|_| "Invalid item")?;
    }
    Ok(ItemId(bytes))
}
fn view(d: SavedDraft, preview: bool) -> Value {
    let content = if preview && d.kind != "image" {
        d.content.text.chars().take(240).collect::<String>()
    } else if preview {
        STANDARD
            .decode(&d.content.text)
            .ok()
            .and_then(|bytes| image::load_from_memory(&bytes).ok())
            .and_then(|img| {
                let thumb = img.thumbnail(240, 140);
                let mut out = std::io::Cursor::new(Vec::new());
                thumb.write_to(&mut out, image::ImageFormat::Png).ok()?;
                Some(STANDARD.encode(out.into_inner()))
            })
            .unwrap_or_default()
    } else {
        d.content.text
    };
    json!({"id":id_text(d.id),"revision":d.revision,"heading":d.content.heading,"text":content,"source":d.content.source,"updated":d.updated_ms,"kind":d.kind,"pinned":d.pinned})
}

pub struct Service {
    store: Store,
    pub prefs: Preferences,
    settings: Arc<RwLock<Preferences>>,
    // Only bounded hashes of recently deleted snapshots; no archive-sized plaintext cache.
    suppressed: HashMap<[u8; 32], [u8; 32]>,
    dir: PathBuf,
    last_error: Option<String>,
    last_saved: i64,
    pub(crate) ignored_clipboard: u32,
    salt: [u8; 32],
    writes: u32,
    pub capture_health: Arc<RwLock<crate::capture::CaptureHealth>>,
    pub clipboard_health: Arc<RwLock<crate::capture::CaptureHealth>>,
}
impl Service {
    pub fn open(dir: PathBuf, settings: Arc<RwLock<Preferences>>) -> Result<Self, String> {
        std::fs::create_dir_all(&dir).map_err(|_| "Could not create local data folder")?;
        let mut store = Store::open(dir.join("lossy.db")).map_err(|e| e.to_string())?;
        let prefs: Preferences = store
            .preference("settings")
            .map_err(|e| e.to_string())?
            .map(|v| serde_json::from_str(&v))
            .transpose()
            .map_err(|_| "Settings are damaged")?
            .unwrap_or_default();
        *settings.write().unwrap() = prefs.clone();
        let salt = if let Some(key) = store.preference("context-key").map_err(|e| e.to_string())? {
            STANDARD
                .decode(key)
                .map_err(|_| "Context key unavailable")?
                .try_into()
                .map_err(|_| "Context key unavailable")?
        } else {
            let mut key = [0; 32];
            getrandom::fill(&mut key).map_err(|_| "Randomness unavailable")?;
            store
                .set_preference("context-key", &STANDARD.encode(key))
                .map_err(|e| e.to_string())?;
            key
        };
        Ok(Self {
            store,
            prefs,
            settings,
            suppressed: HashMap::new(),
            dir,
            last_error: None,
            last_saved: 0,
            ignored_clipboard: 0,
            salt,
            writes: 0,
            capture_health: Arc::new(RwLock::new(crate::capture::CaptureHealth::default())),
            clipboard_health: Arc::new(RwLock::new(crate::capture::CaptureHealth::default())),
        })
    }
    fn context(&self, key: &str) -> [u8; 32] {
        *blake3::keyed_hash(&self.salt, key.as_bytes()).as_bytes()
    }
    pub fn capture(
        &mut self,
        key: String,
        text: String,
        source: String,
        trusted: bool,
    ) -> Result<(), String> {
        if !self.prefs.enabled || self.prefs.paused {
            return Ok(());
        }
        let text = normalize_line_endings(&text);
        if text.len() > 1024 * 1024 || source.len() > 4096 || key.len() > 4096 {
            return Err("Editor content exceeds the capture limit".into());
        }
        let context = self.context(&key);
        if text.is_empty() {
            self.suppressed.remove(&context);
            self.store.finish(context).map_err(|e| e.to_string())?;
            return Ok(());
        }
        let fingerprint = *blake3::hash(text.as_bytes()).as_bytes();
        if self.suppressed.get(&context) == Some(&fingerprint) {
            return Ok(());
        }
        self.suppressed.remove(&context);
        let content = DraftContent {
            heading: text
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("Untitled draft")
                .chars()
                .take(72)
                .collect(),
            text: text.clone(),
            source,
        };
        let previous = self.store.active(context).map_err(|e| e.to_string())?;
        if let Some(old) = previous {
            if old.content.text == text {
                return Ok(());
            }
            if trusted || related_draft(&old.content.text, &text) {
                match self.store.update(old.id, old.revision, &content) {
                    Ok(d) => d,
                    Err(
                        lossy_storage::StoreError::NotFound | lossy_storage::StoreError::Conflict,
                    ) => self
                        .store
                        .create(context, &content)
                        .map_err(|e| e.to_string())?,
                    Err(e) => return Err(e.to_string()),
                }
            } else {
                self.store
                    .create(context, &content)
                    .map_err(|e| e.to_string())?
            }
        } else {
            self.store
                .create(context, &content)
                .map_err(|e| e.to_string())?
        };
        self.last_saved = now();
        self.last_error = None;
        self.writes += 1;
        if self.writes.is_multiple_of(100) {
            self.store.compact(32).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    /// Persist one explicit clipboard copy. Clipboard history is deliberately separate
    /// from draft capture: every copy is a standalone card and never edits a draft.
    pub fn clipboard(
        &mut self,
        text: String,
        kind: String,
        source: String,
        sequence: u32,
    ) -> Result<(), String> {
        if !self.prefs.enabled
            || self.prefs.paused
            || !self.prefs.clipboard
            || sequence == self.ignored_clipboard
        {
            return Ok(());
        }
        if !["clipboard", "image"].contains(&kind.as_str()) {
            return Err("Unsupported clipboard type".into());
        }
        if text.is_empty() || text.len() > 6 * 1024 * 1024 || source.len() > 4096 {
            return Err("Clipboard content exceeds the capture limit".into());
        }
        let heading = if kind == "image" {
            "Copied image".to_owned()
        } else {
            text.lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("Copied text")
                .chars()
                .take(72)
                .collect()
        };
        self.store
            .create_kind(
                self.context("clipboard"),
                &DraftContent {
                    heading,
                    text,
                    source,
                },
                &kind,
            )
            .map_err(|e| e.to_string())?;
        self.last_saved = now();
        self.last_error = None;
        self.writes += 1;
        Ok(())
    }
    fn render(&self, draft: SavedDraft, preview: bool) -> Result<Value, String> {
        let color = self
            .store
            .preference(&format!("color/{}", id_text(draft.id)))
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| "paper".into());
        let mut item = view(draft, preview);
        item["color"] = json!(color);
        Ok(item)
    }
    pub fn request(&mut self, request: Value) -> Result<Value, String> {
        let op = request["op"].as_str().ok_or("Missing operation")?;
        let id = || parse_id(request["id"].as_str().unwrap_or_default());
        match op {
            "status" => Ok(
                json!({"prefs":self.prefs,"recommended_apps":recommended_apps(),"last_saved":self.last_saved,"error":self.last_error,"data_dir":self.dir,"native":*self.capture_health.read().unwrap(),"clipboard_status":*self.clipboard_health.read().unwrap(),"capture":"Supported editable fields only. Browsers need the companion for drafts; protected fields and password managers are excluded."}),
            ),
            "list" => {
                let mut results = Vec::new();
                let order = self
                    .store
                    .preference("board-order")
                    .map_err(|e| e.to_string())?
                    .unwrap_or_else(|| "[]".into());
                let search = request["search"].as_str().unwrap_or("").to_lowercase();
                let filter = request["filter"].as_str().unwrap_or("all");
                let offset = request["offset"].as_u64().unwrap_or(0).min(100000) as usize;
                let mut scanned = 0;
                let mut matched = 0;
                loop {
                    let page = self
                        .store
                        .list_in_order(10, scanned, &order)
                        .map_err(|e| e.to_string())?;
                    if page.is_empty() {
                        break;
                    }
                    scanned += page.len() as u32;
                    for d in page {
                        if filter != "all" && !(filter == "pinned" && d.pinned) && filter != d.kind
                        {
                            continue;
                        }
                        if !search.is_empty()
                            && !format!(
                                "{} {} {}",
                                d.content.heading,
                                d.content.source,
                                if d.kind == "image" {
                                    ""
                                } else {
                                    &d.content.text
                                }
                            )
                            .to_lowercase()
                            .contains(&search)
                        {
                            continue;
                        }
                        matched += 1;
                        if matched <= offset {
                            continue;
                        }
                        results.push(self.render(d, true)?);
                        if results.len() >= 61 {
                            break;
                        }
                    }
                    if results.len() >= 61 {
                        break;
                    }
                }
                Ok(
                    json!({"items":results.iter().take(60).collect::<Vec<_>>(),"more":results.len()>60}),
                )
            }
            "reorder" => {
                let ids = request["ids"].as_array().ok_or("Missing card order")?;
                if ids.len() > 10000 {
                    return Err("Too many cards to arrange at once".into());
                }
                let previous = self
                    .store
                    .preference("board-order")
                    .map_err(|e| e.to_string())?
                    .unwrap_or_else(|| "[]".into());
                let previous: Vec<String> =
                    serde_json::from_str(&previous).map_err(|e| e.to_string())?;
                let mut order = Vec::new();
                let mut seen = std::collections::HashSet::new();
                for value in ids {
                    let value = value.as_str().ok_or("Invalid card ID")?;
                    let value = id_text(parse_id(value)?);
                    if !seen.insert(value.clone()) {
                        return Err("Duplicate card ID".into());
                    }
                    order.push(value);
                }
                // Keep unloaded cards in their existing order, never replace captured content.
                for value in previous {
                    if order.len() < 10000 && seen.insert(value.clone()) {
                        order.push(value);
                    }
                }
                self.store
                    .set_preference(
                        "board-order",
                        &serde_json::to_string(&order).map_err(|e| e.to_string())?,
                    )
                    .map_err(|e| e.to_string())?;
                Ok(json!(true))
            }
            "get" => self.render(self.store.latest(id()?).map_err(|e| e.to_string())?, false),
            "revision" => self.render(
                self.store
                    .revision(
                        id()?,
                        request["revision"].as_i64().ok_or("Missing revision")?,
                    )
                    .map_err(|e| e.to_string())?,
                false,
            ),
            "color" => {
                let item = self.store.latest(id()?).map_err(|e| e.to_string())?;
                let color = request["color"].as_str().ok_or("Choose a box color")?;
                if !["paper", "rose", "peach", "lavender", "sage", "blue"].contains(&color) {
                    return Err("Unknown box color".into());
                }
                self.store
                    .set_preference(&format!("color/{}", id_text(item.id)), color)
                    .map_err(|e| e.to_string())?;
                self.render(item, false)
            }
            "save" => {
                let heading = request["heading"]
                    .as_str()
                    .unwrap_or("Untitled note")
                    .chars()
                    .take(120)
                    .collect();
                let text = request["text"].as_str().ok_or("Missing text")?.to_owned();
                let content = DraftContent {
                    heading,
                    text,
                    source: "My notes".into(),
                };
                let d = if request["id"].is_string() {
                    let old = self.store.latest(id()?).map_err(|e| e.to_string())?;
                    if old.kind != "note" {
                        return Err("Save a recovery copy to keep the captured original".into());
                    }
                    self.store.update(
                        old.id,
                        request["revision"].as_i64().ok_or("Missing revision")?,
                        &content,
                    )
                } else {
                    self.store
                        .create_kind(self.context("notes"), &content, "note")
                }
                .map_err(|e| e.to_string())?;
                self.last_saved = now();
                self.render(d, false)
            }
            "pin" => {
                self.store
                    .pin(id()?, request["pinned"].as_bool().unwrap_or(false))
                    .map_err(|e| e.to_string())?;
                Ok(json!(true))
            }
            "delete" => {
                let id = id()?;
                let old = self.store.latest(id).map_err(|e| e.to_string())?;
                self.store
                    .delete(id, request["revision"].as_i64().ok_or("Missing revision")?)
                    .map_err(|e| e.to_string())?;
                if self.suppressed.len() >= 256 {
                    self.suppressed.clear();
                }
                self.suppressed.insert(
                    old.context,
                    *blake3::hash(old.content.text.as_bytes()).as_bytes(),
                );
                Ok(json!(true))
            }
            "copy" => {
                let d = if let Some(revision) = request["revision"].as_i64() {
                    self.store.revision(id()?, revision)
                } else {
                    self.store.latest(id()?)
                }
                .map_err(|e| e.to_string())?;
                let mut cb = arboard::Clipboard::new().map_err(|_| "Clipboard unavailable")?;
                if d.kind == "image" {
                    let bytes = STANDARD
                        .decode(&d.content.text)
                        .map_err(|_| "Image damaged")?;
                    let image = image::load_from_memory(&bytes)
                        .map_err(|_| "Image damaged")?
                        .into_rgba8();
                    cb.set_image(arboard::ImageData {
                        width: image.width() as usize,
                        height: image.height() as usize,
                        bytes: std::borrow::Cow::Owned(image.into_raw()),
                    })
                    .map_err(|_| "Clipboard busy; try again")?;
                } else {
                    cb.set_text(d.content.text)
                        .map_err(|_| "Clipboard busy; try again")?;
                }
                self.ignored_clipboard = crate::platform::clipboard_sequence();
                Ok(json!(true))
            }
            "settings" => {
                let prefs: Preferences = serde_json::from_value(request["prefs"].clone())
                    .map_err(|_| "Invalid settings")?;
                if prefs.retention_days < 1
                    || prefs.retention_days > 3650
                    || prefs.allowed_apps.len() > 100
                    || prefs.allowed_apps.iter().any(|app| {
                        app.len() > 260
                            || !app.to_ascii_lowercase().ends_with(".exe")
                            || app.contains(['\\', '/', ':', '*', '?', '"', '<', '>', '|'])
                            || app.trim() != app
                    })
                {
                    return Err("Check the retention and application settings".into());
                }
                if prefs.autostart != self.prefs.autostart {
                    crate::platform::startup(prefs.autostart)?;
                }
                self.store
                    .set_preference("settings", &serde_json::to_string(&prefs).unwrap())
                    .map_err(|e| e.to_string())?;
                self.prefs = prefs.clone();
                *self.settings.write().unwrap() = prefs;
                Ok(json!(true))
            }
            "browser_capture" => {
                if !self.prefs.browser_capture
                    || request["private"].as_bool() != Some(false)
                    || request["secure"].as_bool() != Some(false)
                {
                    return Ok(json!(false));
                }
                self.capture(
                    request["context"].as_str().ok_or("Missing context")?.into(),
                    request["text"].as_str().ok_or("Missing text")?.into(),
                    request["source"].as_str().unwrap_or("Browser").into(),
                    true,
                )?;
                Ok(json!(true))
            }
            "backup" => {
                self.backup()?;
                Ok(json!(true))
            }
            "verify" => {
                self.store.verify_integrity().map_err(|e| e.to_string())?;
                Ok(json!(true))
            }
            _ => Err("Unknown operation".into()),
        }
    }
    fn backup(&self) -> Result<(), String> {
        let dir = self.dir.join("backups");
        std::fs::create_dir_all(&dir).map_err(|_| "Backup folder unavailable")?;
        self.store
            .backup(dir.join(format!("lossy-{}.db", now())))
            .map_err(|e| e.to_string())?;
        let mut backups = std::fs::read_dir(&dir)
            .map_err(|_| "Backups unavailable")?
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name().to_string_lossy().starts_with("lossy-")
                    && e.path().extension().is_some_and(|x| x == "db")
            })
            .collect::<Vec<_>>();
        backups.sort_by_key(|e| e.file_name());
        let remove = backups.len().saturating_sub(3);
        for entry in backups.into_iter().take(remove) {
            let _ = std::fs::remove_file(entry.path());
        }
        Ok(())
    }
}

pub fn launch() -> Result<SyncSender<Message>, String> {
    let listener = crate::ipc::listener()?; // Own the singleton before opening the database.
    let settings = Arc::new(RwLock::new(Preferences::default()));
    let mut service = Service::open(directory(), settings.clone());
    let (tx, rx) = mpsc::sync_channel(128);
    let pipe_tx = tx.clone();
    std::thread::spawn(move || {
        for connection in listener.incoming() {
            let Ok(mut stream) = connection else {
                continue;
            };
            let _ = stream.set_nonblocking(true);
            if let Ok(request) = crate::ipc::receive(&mut stream) {
                let (reply_tx, reply_rx) = mpsc::sync_channel(1);
                if pipe_tx.send(Message::Request(request, reply_tx)).is_err() {
                    break;
                }
                let response = match reply_rx.recv_timeout(Duration::from_secs(10)) {
                    Ok(Ok(value)) => json!({"ok":value}),
                    Ok(Err(e)) => json!({"error":e}),
                    Err(_) => json!({"error":"Background process busy"}),
                };
                let _ = crate::ipc::send(&mut stream, &response);
            }
        }
    });
    if let Ok(s) = &service {
        crate::capture::start(tx.clone(), settings.clone(), s.capture_health.clone());
        clipboard_watch(tx.clone(), settings.clone(), s.clipboard_health.clone());
    }
    std::thread::spawn(move || {
        let mut maintenance = Instant::now();
        loop {
            let message = rx.recv_timeout(Duration::from_secs(1));
            match message {
                Ok(Message::Request(req, reply)) => {
                    let result = match &mut service {
                        Ok(s) => s.request(req),
                        Err(e) => {
                            if req["op"] == "status" {
                                Ok(json!({"error":e,"prefs":Preferences::default(),"last_saved":0}))
                            } else {
                                Err(e.clone())
                            }
                        }
                    };
                    let _ = reply.send(result);
                }
                Ok(Message::Capture {
                    context,
                    text,
                    source,
                    trusted,
                    committed,
                }) => {
                    let saved = if let Ok(s) = &mut service {
                        match s.capture(context, text, source, trusted) {
                            Ok(()) => true,
                            Err(e) => {
                                s.last_error = Some(e);
                                false
                            }
                        }
                    } else {
                        false
                    };
                    let _ = committed.send(saved);
                }
                Ok(Message::Clipboard {
                    text,
                    kind,
                    source,
                    sequence,
                }) => {
                    if let Ok(s) = &mut service
                        && let Err(e) = s.clipboard(text, kind, source, sequence)
                    {
                        s.last_error = Some(e);
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                _ => {}
            }
            if maintenance.elapsed() > Duration::from_secs(1800) {
                if let Ok(s) = &mut service {
                    if let Err(e) = s.store.compact(32).and_then(|_| {
                        s.store
                            .retain_since(now() - i64::from(s.prefs.retention_days) * 86_400_000)
                    }) {
                        s.last_error = Some(e.to_string());
                    }
                    if let Err(e) = s.backup() {
                        s.last_error = Some(e);
                    }
                }
                maintenance = Instant::now();
            }
        }
    });
    Ok(tx)
}
fn clipboard_source() -> Option<String> {
    // The clipboard owner identifies the app that performed the copy even if the
    // user switched windows before the next poll. Fall back to the foreground
    // app for copies whose owner has already exited (some OLE/delayed renders).
    if let Some(pid) = crate::platform::clipboard_owner_pid()
        && let Some(exe) = crate::platform::process_exe(pid)
    {
        return Some(exe);
    }
    crate::platform::foreground().map(|(_, exe, _)| exe)
}

fn password_focused(automation: &Option<uiautomation::UIAutomation>) -> bool {
    automation
        .as_ref()
        .and_then(|automation| automation.get_focused_element().ok())
        .is_some_and(|element| element.is_password().ok() == Some(true))
}

fn clipboard_watch(
    tx: SyncSender<Message>,
    settings: Arc<RwLock<Preferences>>,
    health: Arc<RwLock<crate::capture::CaptureHealth>>,
) {
    std::thread::spawn(move || {
        // Clipboard capture must not depend on accessibility. UIA is used only
        // as a best-effort check for a focused password field.
        let mut automation = uiautomation::UIAutomation::new().ok();
        let mut last_uia_attempt = Instant::now();
        let mut handled_sequence = crate::platform::clipboard_sequence();
        let mut retry_sequence = handled_sequence;
        let mut attempts = 0u8;
        let mut previous = String::new();
        loop {
            std::thread::sleep(Duration::from_millis(100));
            if automation.is_none() && last_uia_attempt.elapsed() > Duration::from_secs(30) {
                automation = uiautomation::UIAutomation::new().ok();
                last_uia_attempt = Instant::now();
            }
            let current = crate::platform::clipboard_sequence();
            if current != retry_sequence {
                retry_sequence = current;
                attempts = 0;
            }
            if current == handled_sequence {
                continue;
            }
            let prefs = settings.read().unwrap().clone();
            if !prefs.enabled
                || prefs.paused
                || !prefs.clipboard
                || !crate::platform::desktop_available()
            {
                handled_sequence = current;
                continue;
            }
            let Some(exe) = clipboard_source() else {
                crate::capture::report(&health, "", "No source app for the clipboard copy");
                if attempts >= 8 {
                    handled_sequence = current;
                } else {
                    attempts += 1;
                }
                continue;
            };
            if let Some(reason) = crate::capture::clipboard_exclusion(&exe) {
                crate::capture::report(&health, &exe, reason);
                handled_sequence = current;
                continue;
            }
            // A password field was focused when the poll happened. This is a
            // best-effort guard; explicit copies from ordinary fields are kept.
            if password_focused(&automation) {
                crate::capture::report(&health, &exe, "Password field was focused; copy skipped");
                handled_sequence = current;
                continue;
            }
            let Ok(mut clipboard) = arboard::Clipboard::new() else {
                crate::capture::report(&health, &exe, "Clipboard busy; retrying");
                if attempts >= 8 {
                    handled_sequence = current;
                } else {
                    attempts += 1;
                }
                continue;
            };
            // Prefer text when the copy offers both (for example spreadsheet
            // cells). Bitmap-only copies still become image cards.
            let text = clipboard.get_text().ok().filter(|text| !text.is_empty());
            let result = if let Some(text) = text {
                Some((text, "clipboard"))
            } else if let Ok(image) = clipboard.get_image() {
                if image.width.saturating_mul(image.height) > 16_000_000 {
                    crate::capture::report(&health, &exe, "Copied image exceeds the pixel limit");
                    handled_sequence = current;
                    continue;
                }
                let rgba = image::RgbaImage::from_raw(
                    image.width as u32,
                    image.height as u32,
                    image.bytes.to_vec(),
                );
                rgba.and_then(|rgba| {
                    let mut out = std::io::Cursor::new(Vec::new());
                    image::DynamicImage::ImageRgba8(rgba)
                        .write_to(&mut out, image::ImageFormat::Png)
                        .ok()?;
                    Some((STANDARD.encode(out.into_inner()), "image"))
                })
            } else {
                None
            };
            let Some((text, kind)) = result else {
                // The clipboard can be held briefly by the source app or contain
                // a format Lossy cannot read. Retry a few polls before dropping it.
                if attempts >= 8 {
                    crate::capture::report(
                        &health,
                        &exe,
                        "No supported text or bitmap clipboard format",
                    );
                    handled_sequence = current;
                } else {
                    attempts += 1;
                }
                continue;
            };
            // The sequence must still be the same copy that was read. Focus is
            // intentionally not rechecked: switching windows after Ctrl+C must
            // not lose the copy.
            if crate::platform::clipboard_sequence() != current {
                crate::capture::report(&health, &exe, "Clipboard changed while reading; retrying");
                continue;
            }
            if text.is_empty() || text.len() > 6 * 1024 * 1024 {
                crate::capture::report(
                    &health,
                    &exe,
                    "Clipboard is empty or exceeds the size limit",
                );
                handled_sequence = current;
                continue;
            }
            let hash = blake3::hash(text.as_bytes()).to_hex().to_string();
            if hash == previous {
                handled_sequence = current;
                continue;
            }
            previous = hash;
            crate::capture::report(&health, &exe, "Clipboard snapshot sent to the local writer");
            let _ = tx.send(Message::Clipboard {
                text,
                kind: kind.into(),
                source: format!("Clipboard · {}", exe.trim_end_matches(".exe")),
                sequence: current,
            });
            handled_sequence = current;
        }
    });
}
