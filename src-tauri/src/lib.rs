mod accessibility;
mod audio;
mod config;
mod hotkey;
mod meeting;
mod meeting_api;
mod meeting_store;
mod pipeline;
mod wav_writer;

use config::Config;
use serde::Serialize;
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
use tauri_plugin_autostart::ManagerExt;

pub(crate) struct AppState {
    pub(crate) config: Config,
    config_path: PathBuf,
    hotkey_handle: Option<hotkey::HotkeyHandle>,
    pub(crate) recording: bool,
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AutostartStatus {
    enabled: bool,
    installed: bool,
}

/// The login item registers whatever `current_exe()` points at, so enabling it
/// from a dev build or from the build directory silently pins the wrong binary.
fn running_from_applications() -> bool {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().contains("/Applications/"))
        .unwrap_or(false)
}

#[tauri::command]
fn get_autostart(app: AppHandle) -> AutostartStatus {
    AutostartStatus {
        enabled: app.autolaunch().is_enabled().unwrap_or(false),
        installed: running_from_applications(),
    }
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    if enabled && !running_from_applications() {
        return Err("VoiceBox must be installed to /Applications first".into());
    }

    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    result.map_err(|e| e.to_string())?;

    log::info!(
        "Autostart {} for {:?}",
        if enabled { "enabled" } else { "disabled" },
        std::env::current_exe().unwrap_or_default()
    );
    Ok(())
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
    // The meeting recorder owns the microphone while it runs.
    if meeting::is_recording(&app_handle) {
        log::info!("Ignoring dictation hotkey during meeting recording");
        return;
    }

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
            hide_overlay_unless_meeting(&app);
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
            hide_overlay_unless_meeting(&app);
            let _ = app.emit("voicebox:state", serde_json::json!({"state": "idle"}));
        }
    }
}

/// A dictation that was already in flight when a meeting recording started
/// must not hide the overlay out from under the meeting's recording pill.
fn hide_overlay_unless_meeting(app: &AppHandle) {
    if !meeting::is_recording(app) {
        hide_overlay(app);
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

pub(crate) fn show_overlay(app: &AppHandle) {
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

pub(crate) fn hide_overlay(app: &AppHandle) {
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

fn show_meetings(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("meetings") {
        let _ = win.show();
        let _ = win.set_focus();
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

/// Headless Meeting Mode processing of an existing WAV file:
///
/// ```text
/// voicebox --meeting-file path/to/recording.wav
/// ```
///
/// Copies the WAV into a new meeting folder and runs the same
/// upload → diarize → enrich pipeline the in-app Stop button runs.
fn run_headless_meeting(cfg: &Config, wav_path: &str) {
    let source = PathBuf::from(wav_path);
    if !source.is_file() {
        eprintln!("No such file: {}", wav_path);
        std::process::exit(2);
    }

    let (meeting_id, dir) = match meeting_store::create_meeting_dir(cfg) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    let mut doc = meeting_store::new_doc(&meeting_id, now_millis());
    if let Err(e) = std::fs::copy(&source, dir.join(&doc.audio_file)) {
        eprintln!("Copying audio failed: {}", e);
        std::process::exit(1);
    }
    let data_bytes = std::fs::metadata(dir.join(&doc.audio_file)).map(|m| m.len()).unwrap_or(0);
    doc.duration_sec = data_bytes.saturating_sub(44) as f64 / (cfg.audio.sample_rate as f64 * 2.0);
    if let Err(e) = meeting_store::save_doc(&dir, &doc) {
        eprintln!("{}", e);
        std::process::exit(1);
    }

    eprintln!("meeting:  {} ({})", meeting_id, dir.display());
    eprintln!("input:    {} ({} bytes, ~{:.1} min)", wav_path, data_bytes, doc.duration_sec / 60.0);

    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let total = std::time::Instant::now();
    let result = runtime.block_on(meeting::run_processing(cfg, &meeting_id, |event| match event {
        meeting::ProcessEvent::Uploading(p) => eprintln!("stage:    uploading {:.0}%", p * 100.0),
        meeting::ProcessEvent::Transcribing => eprintln!("stage:    transcribing"),
        meeting::ProcessEvent::Formatting => eprintln!("stage:    formatting"),
    }));

    match result {
        Ok(doc) => {
            eprintln!("total:    {}ms", total.elapsed().as_millis());
            eprintln!(
                "result:   {} turns, {} speakers, title: {}",
                doc.turns.len(),
                doc.speakers.len(),
                doc.title.as_deref().unwrap_or("(none)")
            );
            eprintln!("---");
            println!("{}", dir.join("meeting.md").display());
        }
        Err(e) => {
            eprintln!("Meeting processing failed: {}", e);
            std::process::exit(1);
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_log();

    let (cfg, cfg_path) = config::load();

    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--meeting-file") {
        match args.get(i + 1) {
            Some(path) => return run_headless_meeting(&cfg, path),
            None => {
                eprintln!("--meeting-file requires a path");
                std::process::exit(2);
            }
        }
    }

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
            get_autostart,
            set_autostart,
            meeting::start_meeting,
            meeting::stop_meeting,
            meeting::get_meeting_state,
            meeting::retry_meeting,
            meeting::list_meetings,
            meeting::load_meeting,
            meeting::rename_speaker,
            meeting::open_meetings_folder,
        ])
        .setup(move |app| {
            let app_handle = app.handle().clone();

            app_handle.plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                Some(vec!["--autostart"]),
            ))?;

            // "main" is declared hidden so a login-item launch can never flash
            // it: config windows are created before setup runs, so hiding it
            // here instead would race — worst of all on a cold boot, which is
            // exactly when launchd starts us.
            if !std::env::args().any(|a| a == "--autostart") {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                }
            }

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
            let toggle_meeting_item =
                MenuItemBuilder::with_id("toggle_meeting", "Start Meeting Recording").build(app)?;
            let open_meetings_item =
                MenuItemBuilder::with_id("open_meetings", "Meetings…").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let tray_menu = MenuBuilder::new(app)
                .item(&show_settings_item)
                .item(&toggle_meeting_item)
                .item(&open_meetings_item)
                .separator()
                .item(&quit_item)
                .build()?;

            // The meeting module keeps the toggle item's handle so it can flip
            // the label between Start/Stop at runtime.
            meeting::init(&app_handle, toggle_meeting_item);

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
                    "toggle_meeting" => {
                        let result = if meeting::is_recording(app) {
                            meeting::stop(app)
                        } else {
                            meeting::start(app).inspect(|_| show_meetings(app))
                        };
                        if let Err(e) = result {
                            log::error!("[meeting] tray toggle failed: {}", e);
                            let _ = app.emit(
                                "voicebox:meeting-state",
                                serde_json::json!({
                                    "state": "error", "meetingId": "", "message": e,
                                }),
                            );
                            show_meetings(app);
                        }
                    }
                    "open_meetings" => show_meetings(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click { .. } = event {
                        show_settings(tray.app_handle());
                    }
                })
                .build(app)?;

            // Hide windows on close (keep app running)
            for label in ["main", "meetings"] {
                if let Some(win) = app.get_webview_window(label) {
                    let win_clone = win.clone();
                    win.on_window_event(move |event| {
                        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                            api.prevent_close();
                            let _ = win_clone.hide();
                        }
                    });
                }
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
