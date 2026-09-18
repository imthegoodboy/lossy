#![windows_subsystem = "windows"]
mod agent;
mod capture;
mod ipc;
mod platform;
#[cfg(test)]
mod tests;

use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    sync::mpsc,
    time::Duration,
};
use tauri::{
    Manager,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

fn ensure_agent() -> Result<(), String> {
    if ipc::request(&json!({"op":"status"})).is_ok() {
        return Ok(());
    }
    platform::spawn_agent()?;
    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(100));
        if ipc::request(&json!({"op":"status"})).is_ok() {
            return Ok(());
        }
    }
    Err("Lossy could not start its background process".into())
}

#[tauri::command]
async fn request(payload: Value) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || ipc::request(&payload))
        .await
        .map_err(|_| "Background request failed".to_string())?
}
#[tauri::command]
fn open_data_folder() -> Result<(), String> {
    std::process::Command::new("explorer.exe")
        .arg(agent::directory())
        .spawn()
        .map_err(|_| "Could not open folder")?;
    Ok(())
}

#[tauri::command]
fn setup_browser(app: tauri::AppHandle) -> Result<String, String> {
    register_browser_host()?;
    let bundled = app
        .path()
        .resource_dir()
        .map_err(|_| "Resources unavailable")?
        .join("browser");
    let path = if bundled.join("manifest.json").exists() {
        bundled
    } else {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../integrations/browser")
    };
    std::process::Command::new("explorer.exe")
        .arg(&path)
        .spawn()
        .map_err(|_| "Could not open companion folder")?;
    Ok("Companion registered. In Chrome or Edge, open Extensions, enable Developer mode, choose Load unpacked, and select the browser folder that just opened. Then enable Lossy on each website using its toolbar button.".into())
}

fn register_browser_host() -> Result<(), String> {
    let dir = agent::directory();
    std::fs::create_dir_all(&dir).map_err(|_| "Data folder unavailable")?;
    let manifest = dir.join("browser-host.json");
    let host = json!({"name":"app.lossy.companion","description":"Lossy local draft recovery","path":std::env::current_exe().map_err(|_| "Executable unavailable")?,"type":"stdio","allowed_origins":["chrome-extension://bbebeppoampdkokfpfiihnldhhjegoej/"]});
    std::fs::write(&manifest, serde_json::to_vec_pretty(&host).unwrap())
        .map_err(|_| "Could not write companion registration")?;
    for browser in ["Google\\Chrome", "Microsoft\\Edge"] {
        let path = format!("Software\\{browser}\\NativeMessagingHosts\\app.lossy.companion");
        let (key, _) = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
            .create_subkey(path)
            .map_err(|_| "Could not register browser companion")?;
        key.set_value("", &manifest.to_string_lossy().as_ref())
            .map_err(|_| "Could not register browser companion")?;
    }
    Ok(())
}

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.iter().any(|v| v == "--repair-install") {
        // The installer has stopped the old agent. Preserve the archive and consent;
        // repair only integrations the user previously enabled, without a window.
        let result = (|| -> Result<(), String> {
            let dir = agent::directory();
            if !dir.join("lossy.db").exists() {
                return Ok(());
            }
            let service = agent::Service::open(dir.clone(), Default::default())?;
            if service.prefs.autostart {
                platform::startup(true)?;
            }
            if dir.join("browser-host.json").exists() {
                register_browser_host()?;
            }
            Ok(())
        })();
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    }
    if args
        .iter()
        .any(|v| v == "--native-host" || v.starts_with("chrome-extension://"))
    {
        native_host();
        return;
    }
    if args.iter().any(|v| v == "--self-test") {
        self_test();
        return;
    }
    if args.iter().any(|v| v == "--uninstall-startup") {
        let _ = platform::startup(false);
        return;
    }
    let background = args.iter().any(|v| v == "--agent");
    let sender = if background {
        match agent::launch() {
            Ok(tx) => Some(tx),
            Err(_) => return,
        }
    } else {
        let _ = ensure_agent();
        None
    };
    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            request,
            open_data_folder,
            setup_browser
        ])
        .setup(move |app| {
            if let Some(tx) = sender {
                let open = MenuItem::with_id(app, "open", "Open Lossy", true, None::<&str>)?;
                let pause =
                    MenuItem::with_id(app, "pause", "Pause / resume saving", true, None::<&str>)?;
                let quit = MenuItem::with_id(app, "quit", "Quit Lossy", true, None::<&str>)?;
                let browser = MenuItem::with_id(
                    app,
                    "browser",
                    "Set up browser companion",
                    true,
                    None::<&str>,
                )?;
                let menu = Menu::with_items(app, &[&open, &pause, &browser, &quit])?;
                TrayIconBuilder::new()
                    .icon(app.default_window_icon().unwrap().clone())
                    .tooltip("Lossy · local draft recovery")
                    .menu(&menu)
                    .on_menu_event(move |app, event| match event.id.as_ref() {
                        "open" => platform::open_ui(),
                        "browser" => {
                            let _ = setup_browser(app.clone());
                        }
                        "pause" => {
                            let tx = tx.clone();
                            std::thread::spawn(move || {
                                let (reply, rx) = mpsc::sync_channel(1);
                                let _ =
                                    tx.send(agent::Message::Request(json!({"op":"status"}), reply));
                                if let Ok(Ok(status)) = rx.recv_timeout(Duration::from_secs(3))
                                    && let Ok(mut prefs) =
                                        serde_json::from_value::<agent::Preferences>(
                                            status["prefs"].clone(),
                                        )
                                {
                                    prefs.paused = !prefs.paused;
                                    let (reply, _) = mpsc::sync_channel(1);
                                    let _ = tx.send(agent::Message::Request(
                                        json!({"op":"settings","prefs":prefs}),
                                        reply,
                                    ));
                                }
                            });
                        }
                        "quit" => app.exit(0),
                        _ => {}
                    })
                    .on_tray_icon_event(|_, event| {
                        if matches!(
                            event,
                            TrayIconEvent::Click {
                                button: MouseButton::Left,
                                button_state: MouseButtonState::Up,
                                ..
                            }
                        ) {
                            platform::open_ui();
                        }
                    })
                    .build(app)?;
            } else {
                tauri::WebviewWindowBuilder::new(
                    app,
                    "main",
                    tauri::WebviewUrl::App("index.html".into()),
                )
                .title("Lossy")
                // Let WebView2 deliver HTML drag events for card rearrangement.
                .disable_drag_drop_handler()
                .inner_size(1120.0, 780.0)
                .min_inner_size(760.0, 540.0)
                .build()?;
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Lossy could not create its window");
    app.run(move |_, event| {
        if background
            && let tauri::RunEvent::ExitRequested {
                api, code: None, ..
            } = event
        {
            api.prevent_exit();
        }
    });
}

fn native_host() {
    if ensure_agent().is_err() {
        return;
    }
    let mut input = std::io::stdin();
    let mut output = std::io::stdout();
    loop {
        let mut length = [0; 4];
        if input.read_exact(&mut length).is_err() {
            break;
        }
        let size = u32::from_le_bytes(length) as usize;
        if size > 1024 * 1024 {
            break;
        }
        let mut bytes = vec![0; size];
        if input.read_exact(&mut bytes).is_err() {
            break;
        }
        let response = match serde_json::from_slice::<Value>(&bytes) {
            Ok(request) if request["op"] == "browser_capture" => match ipc::request(&request) {
                Ok(value) => json!({"ok":value}),
                Err(e) => json!({"error":e}),
            },
            _ => json!({"error":"Unsupported companion request"}),
        };
        let bytes = serde_json::to_vec(&response).unwrap();
        if output
            .write_all(&(bytes.len() as u32).to_le_bytes())
            .and_then(|_| output.write_all(&bytes))
            .and_then(|_| output.flush())
            .is_err()
        {
            break;
        }
    }
}

fn self_test() {
    // No OS capture or startup registration. Exercise the actual agent command path in isolation.
    let dir = tempfile::tempdir().unwrap();
    let settings = std::sync::Arc::new(std::sync::RwLock::new(agent::Preferences::default()));
    let mut service = agent::Service::open(dir.path().to_owned(), settings).unwrap();
    let first = service
        .request(json!({"op":"save","heading":"Recovery test","text":"Synthetic original"}))
        .unwrap();
    let second=service.request(json!({"op":"save","id":first["id"],"revision":first["revision"],"heading":"Recovery test","text":"Synthetic continued"})).unwrap();
    assert!(
        service
            .request(json!({"op":"save","id":first["id"],"revision":1,"text":"Stale"}))
            .is_err()
    );
    let item = service
        .request(json!({"op":"get","id":first["id"]}))
        .unwrap();
    assert_eq!(item["text"], "Synthetic continued");
    service.request(json!({"op":"verify"})).unwrap();
    drop(service);
    let mut reopened = agent::Service::open(
        dir.path().to_owned(),
        std::sync::Arc::new(std::sync::RwLock::new(agent::Preferences::default())),
    )
    .unwrap();
    assert_eq!(
        reopened
            .request(json!({"op":"get","id":first["id"]}))
            .unwrap(),
        second
    );
    reopened
        .request(json!({"op":"delete","id":first["id"],"revision":second["revision"]}))
        .unwrap();
    assert!(
        reopened
            .request(json!({"op":"get","id":first["id"]}))
            .is_err()
    );
    println!("Lossy agent end-to-end self-test passed");
}
