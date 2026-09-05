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
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            enabled: false,
            paused: false,
            clipboard: true,
            autostart: false,
            retention_days: 30,
            allowed_apps: vec![
                "notepad.exe".into(),
                "mspaint.exe".into(),
                "snippingtool.exe".into(),
                "explorer.exe".into(),
                "cursor.exe".into(),
                "code.exe".into(),
            ],
            browser_capture: true,
        }
    }
}

pub enum Message {
    Request(Value, SyncSender<Result<Value, String>>),
    Capture {
        context: String,
        text: String,
        source: String,
        trusted: bool,
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
    ignored_clipboard: u32,
    salt: [u8; 32],
    writes: u32,
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
            if trusted || text.starts_with(&old.content.text) || old.content.text.starts_with(&text)
            {
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
                json!({"prefs":self.prefs,"last_saved":self.last_saved,"error":self.last_error,"data_dir":self.dir,"capture":"Native capture: allowed apps only. Browsers use the companion. Uncertain native edits may become separate cards."}),
            ),
            "list" => {
                let mut results = Vec::new();
                let search = request["search"].as_str().unwrap_or("").to_lowercase();
                let filter = request["filter"].as_str().unwrap_or("all");
                let offset = request["offset"].as_u64().unwrap_or(0).min(100000) as usize;
                let mut scanned = 0;
                let mut matched = 0;
                loop {
                    let page = self.store.list(10, scanned).map_err(|e| e.to_string())?;
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
    crate::capture::start(tx.clone(), settings.clone());
    clipboard_watch(tx.clone(), settings);
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
                }) => {
                    if let Ok(s) = &mut service
                        && let Err(e) = s.capture(context, text, source, trusted)
                    {
                        s.last_error = Some(e);
                    }
                }
                Ok(Message::Clipboard {
                    text,
                    kind,
                    source,
                    sequence,
                }) => {
                    if let Ok(s) = &mut service
                        && s.prefs.enabled
                        && !s.prefs.paused
                        && s.prefs.clipboard
                        && sequence != s.ignored_clipboard
                    {
                        let heading = if kind == "image" {
                            "Copied image".into()
                        } else {
                            text.lines()
                                .next()
                                .unwrap_or("Copied text")
                                .chars()
                                .take(72)
                                .collect()
                        };
                        match s.store.create_kind(
                            s.context("clipboard"),
                            &DraftContent {
                                heading,
                                text,
                                source,
                            },
                            &kind,
                        ) {
                            Ok(_) => {
                                s.last_saved = now();
                                s.last_error = None;
                            }
                            Err(e) => s.last_error = Some(e.to_string()),
                        }
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
fn clipboard_watch(tx: SyncSender<Message>, settings: Arc<RwLock<Preferences>>) {
    std::thread::spawn(move || {
        let Ok(automation) = uiautomation::UIAutomation::new() else {
            return;
        };
        let mut sequence = crate::platform::clipboard_sequence();
        let mut previous = String::new();
        loop {
            std::thread::sleep(Duration::from_millis(100));
            let current = crate::platform::clipboard_sequence();
            if current == sequence {
                continue;
            }
            let prefs = settings.read().unwrap().clone();
            if !prefs.enabled
                || prefs.paused
                || !prefs.clipboard
                || !crate::platform::desktop_available()
            {
                sequence = current;
                continue;
            }
            let Some((_, exe, _)) = crate::platform::foreground() else {
                sequence = current;
                continue;
            };
            // Clipboard capture uses the same explicit native-app allowlist. Browser text/images
            // are excluded rather than guessing private mode. Use an allowed image editor
            // or Snipping Tool for image copies; the companion handles browser drafts only.
            if !crate::capture::allowed(&exe, &prefs) {
                sequence = current;
                continue;
            }
            let safe = automation.get_focused_element().ok().is_some_and(|el| {
                el.is_password().ok() == Some(false)
                    && !crate::capture::sensitive(&el.get_name().unwrap_or_default())
            });
            if !safe {
                sequence = current;
                continue;
            }
            let Ok(mut clipboard) = arboard::Clipboard::new() else {
                continue;
            };
            let result = if let Ok(image) = clipboard.get_image() {
                if image.width.saturating_mul(image.height) > 16_000_000 {
                    sequence = current;
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
                clipboard.get_text().ok().map(|text| (text, "clipboard"))
            };
            if let Some((text, kind)) = result {
                sequence = current;
                if text.is_empty() || text.len() > 6 * 1024 * 1024 {
                    continue;
                }
                let hash = blake3::hash(text.as_bytes()).to_hex().to_string();
                if hash == previous {
                    continue;
                }
                previous = hash;
                let _ = tx.send(Message::Clipboard {
                    text,
                    kind: kind.into(),
                    source: format!("Clipboard · {}", exe.trim_end_matches(".exe")),
                    sequence,
                });
            }
        }
    });
}
