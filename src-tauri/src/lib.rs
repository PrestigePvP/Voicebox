mod accessibility;
mod audio;
mod config;
mod hotkey;
mod pipeline;

use config::Config;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    webview::WebviewWindowBuilder,
    AppHandle, Emitter, Manager, WebviewUrl,
};

struct AppState {
    config: Config,
    config_path: PathBuf,
    hotkey_handle: Option<hotkey::HotkeyHandle>,
    recording: bool,
    capture_handle: Option<audio::AudioCaptureHandle>,
}

// AudioCaptureHandle contains a JoinHandle which is Send
unsafe impl Send for AppState {}
unsafe impl Sync for AppState {}

#[tauri::command]
fn get_config(state: tauri::State<'_, Arc<Mutex<AppState>>>) -> Config {
    state.lock().unwrap().config.clone()
}

#[tauri::command]
fn save_config(
    cfg: Config,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
    app_handle: AppHandle,
) -> Result<(), String> {
    let mut s = state.lock().unwrap();
    config::save(&s.config_path, &cfg)?;

    let hotkey_changed = cfg.hotkey.record != s.config.hotkey.record;
    s.config = cfg.clone();

    if hotkey_changed {
        s.hotkey_handle.take();
        drop(s);
        let handle = register_hotkey(&cfg.hotkey.record, app_handle);
        let mut s = state.lock().unwrap();
        s.hotkey_handle = handle;
    }

    Ok(())
}

#[tauri::command]
fn get_config_path(state: tauri::State<'_, Arc<Mutex<AppState>>>) -> String {
    state.lock().unwrap().config_path.display().to_string()
}

fn register_hotkey(combo: &str, app_handle: AppHandle) -> Option<hotkey::HotkeyHandle> {
    let app_down = app_handle.clone();
    let app_up = app_handle.clone();

    let on_down: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        on_hotkey_down(app_down.clone());
    });

    let on_up: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        on_hotkey_up(app_up.clone());
    });

    match hotkey::register(combo, on_down, on_up) {
        Ok(handle) => {
            log::info!("Registered hotkey: {}", combo);
            Some(handle)
        }
        Err(e) => {
            log::error!("Failed to register hotkey {:?}: {}", combo, e);
            None
        }
    }
}

fn on_hotkey_down(app_handle: AppHandle) {
    let state = app_handle.state::<Arc<Mutex<AppState>>>();
    let mut s = state.lock().unwrap();

    if s.recording {
        return;
    }
    s.recording = true;

    let config = s.config.clone();
    drop(s);

    let focus_ctx = accessibility::get_focus_context();

    if let Some(ref icon_b64) = focus_ctx.icon_base64 {
        log::info!("Emitting app icon ({}b base64)", icon_b64.len());
        let _ = app_handle.emit("voicebox:icon", icon_b64.clone());
    }

    let (capture_handle, chunk_rx) = match audio::start_capture(
        config.audio.sample_rate,
        config.audio.channels,
        config.audio.chunk_size,
        app_handle.clone(),
    ) {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to start audio capture: {}", e);
            let mut s = state.lock().unwrap();
            s.recording = false;
            return;
        }
    };

    {
        let mut s = state.lock().unwrap();
        s.capture_handle = Some(capture_handle);
    }

    show_overlay(&app_handle);

    let _ = app_handle.emit(
        "voicebox:state",
        serde_json::json!({
            "state": "recording",
            "startTime": now_millis(),
        }),
    );

    let app = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        run_pipeline(app, config, focus_ctx, chunk_rx).await;
    });
}

fn on_hotkey_up(app_handle: AppHandle) {
    let state = app_handle.state::<Arc<Mutex<AppState>>>();
    let mut s = state.lock().unwrap();

    if !s.recording {
        return;
    }

    if let Some(ref handle) = s.capture_handle {
        handle.stop();
    }
    let _capture = s.capture_handle.take();
    s.recording = false;
    drop(s);
    drop(_capture);

    let _ = app_handle.emit(
        "voicebox:state",
        serde_json::json!({
            "state": "processing",
            "stage": "stt",
        }),
    );
}

async fn run_pipeline(
    app: AppHandle,
    config: Config,
    focus_ctx: accessibility::FocusContext,
    chunk_rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
) {
    let (server_url, token) = if config.provider.mode == "local" {
        (config.local.server_url.clone(), config.local.token.clone())
    } else {
(config.cloud.worker_url.clone(), config.cloud.token.clone())
    };

    let streaming_stt = config.beta.streaming_stt;
    let app_stage = app.clone();
    let app_partial = app.clone();
    let result = pipeline::run(
        &server_url,
        &token,
        pipeline::AudioParams {
            sample_rate: config.audio.sample_rate,
            channels: config.audio.channels,
            encoding: "pcm_s16le".into(),
        },
        pipeline::FocusContext::from(&focus_ctx),
        streaming_stt,
        chunk_rx,
        move |stage| {
            let _ = app_stage.emit(
                "voicebox:state",
                serde_json::json!({
                    "state": "processing",
                    "stage": stage,
                }),
            );
        },
        move |text| {
            let _ = app_partial.emit("voicebox:partial", text);
        },
    )
    .await;

    match result {
        Ok(result) => {
            if let Err(e) = write_clipboard(&result.formatted) {
                log::error!("Clipboard error: {}", e);
            }

            if focus_ctx.pid > 0 {
                accessibility::paste_into_app(focus_ctx.pid);
            }

            let _ = app.emit("voicebox:state", serde_json::json!({"state": "copied"}));
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            hide_overlay(&app);
            let _ = app.emit("voicebox:state", serde_json::json!({"state": "idle"}));
        }
        Err(e) => {
            log::error!("Pipeline error: {}", e);
            let _ = app.emit(
                "voicebox:state",
                serde_json::json!({
                    "state": "error",
                    "message": e,
                }),
            );
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            hide_overlay(&app);
            let _ = app.emit("voicebox:state", serde_json::json!({"state": "idle"}));
        }
    }
}

fn write_clipboard(text: &str) -> Result<(), String> {
    use std::process::{Command, Stdio};

    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn pbcopy: {}", e))?;

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(text.as_bytes())
        .map_err(|e| format!("Failed to write to pbcopy: {}", e))?;

    child
        .wait()
        .map_err(|e| format!("pbcopy failed: {}", e))?;

    Ok(())
}

fn show_overlay(app: &AppHandle) {
    if let Some(overlay) = app.get_webview_window("overlay") {
        let state = app.state::<Arc<Mutex<AppState>>>();
        let position = state.lock().unwrap().config.overlay_position.clone();

        let monitor = cursor_monitor(&overlay)
            .or_else(|| overlay.primary_monitor().ok().flatten());

        if let Some(monitor) = monitor {
            let scale = monitor.scale_factor();
            let pos = monitor.position();
            let size = monitor.size();
            let mon_x = pos.x as f64 / scale;
            let mon_y = pos.y as f64 / scale;
            let mon_w = size.width as f64 / scale;
            let mon_h = size.height as f64 / scale;
            let ov_w = 320.0;
            let ov_h = 120.0;

            let (x, y) = match position.as_str() {
                "bottom_left" => (mon_x, mon_y + mon_h - ov_h),
                "bottom_center" => (mon_x + (mon_w - ov_w) / 2.0, mon_y + mon_h - ov_h),
                "bottom_right" => (mon_x + mon_w - ov_w, mon_y + mon_h - ov_h),
                _ => (mon_x + (mon_w - ov_w) / 2.0, mon_y),
            };

            let _ = overlay.set_position(tauri::Position::Logical(
                tauri::LogicalPosition::new(x, y),
            ));
        }
        let _ = overlay.show();
    }
}

#[cfg(target_os = "macos")]
fn cursor_monitor(window: &tauri::WebviewWindow) -> Option<tauri::Monitor> {
    use core_graphics::event::CGEvent;
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).ok()?;
    let event = CGEvent::new(source).ok()?;
    let cursor = event.location();

    let monitors = window.available_monitors().ok()?;
    for monitor in monitors {
        let scale = monitor.scale_factor();
        let pos = monitor.position();
        let size = monitor.size();
        let x = pos.x as f64 / scale;
        let y = pos.y as f64 / scale;
        let w = size.width as f64 / scale;
        let h = size.height as f64 / scale;

        if cursor.x >= x && cursor.x < x + w && cursor.y >= y && cursor.y < y + h {
            return Some(monitor);
        }
    }
    None
}

#[cfg(not(target_os = "macos"))]
fn cursor_monitor(_window: &tauri::WebviewWindow) -> Option<tauri::Monitor> {
    None
}

fn hide_overlay(app: &AppHandle) {
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.hide();
    }
}

fn show_settings(app: &AppHandle) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.center();
        let _ = main.show();
        let _ = main.set_focus();
    }
}

fn init_log() {
    let dir = dirs::home_dir()
        .map(|h| h.join(".config").join("voicebox"))
        .unwrap_or_else(|| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&dir);

    let log_path = dir.join("voicebox.log");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .expect("Failed to open log file");

    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Info)
        .format(|buf, record| {
            writeln!(
                buf,
                "{} [{}] {}",
                chrono_timestamp(),
                record.level(),
                record.args()
            )
        })
        .target(env_logger::Target::Pipe(Box::new(file)))
        .init();
}

fn chrono_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    let secs = now.as_secs() as libc::time_t;
    let millis = now.subsec_millis();
    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    unsafe { libc::localtime_r(&secs, &mut tm) };
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        tm.tm_hour, tm.tm_min, tm.tm_sec, millis
    )
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_log();

    let (cfg, cfg_path) = config::load();
    log::info!("VoiceBox ready (hotkey: {})", cfg.hotkey.record);

    let state = Arc::new(Mutex::new(AppState {
        config: cfg.clone(),
        config_path: cfg_path,
        hotkey_handle: None,
        recording: false,
        capture_handle: None,
    }));

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(state.clone())
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            get_config_path,
        ])
        .setup(move |app| {
            let app_handle = app.handle().clone();

            // Create overlay window
            let _overlay = WebviewWindowBuilder::new(
                &app_handle,
                "overlay",
                WebviewUrl::App("index.html".into()),
            )
            .title("VoiceBox Overlay")
            .inner_size(320.0, 120.0)
            .decorations(false)
            .transparent(true)
            .background_color(tauri::window::Color(0, 0, 0, 0))
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(false)
            .focused(false)
            .resizable(false)
            .build()?;

            // Create tray icon
            let show_settings_item =
                MenuItemBuilder::with_id("show_settings", "Show Settings").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let tray_menu = MenuBuilder::new(app)
                .item(&show_settings_item)
                .separator()
                .item(&quit_item)
                .build()?;

            let _tray = TrayIconBuilder::new()
                .icon(
                    app.default_window_icon()
                        .cloned()
                        .unwrap_or_else(|| {
                            let bytes = include_bytes!("../icons/32x32.png");
                            Image::from_bytes(bytes).expect("Failed to load tray icon")
                        }),
                )
                .menu(&tray_menu)
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "show_settings" => show_settings(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click { .. } = event {
                        show_settings(tray.app_handle());
                    }
                })
                .build(app)?;

            // Hide main window on close (keep app running)
            if let Some(main) = app.get_webview_window("main") {
                let main_clone = main.clone();
                main.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = main_clone.hide();
                    }
                });
            }

            // Register hotkey
            let hotkey_handle = register_hotkey(&cfg.hotkey.record, app_handle);
            {
                let mut s = state.lock().unwrap();
                s.hotkey_handle = hotkey_handle;
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
