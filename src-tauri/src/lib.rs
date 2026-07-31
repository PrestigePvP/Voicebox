mod accessibility;
mod audio;
mod config;
mod hotkey;
mod local_engine;
mod models;
mod pipeline;

use config::Config;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use whisper_rs::{WhisperContext, WhisperContextParameters};
use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    webview::WebviewWindowBuilder,
    AppHandle, Emitter, Manager, WebviewUrl,
};

#[derive(Clone, Copy, PartialEq)]
enum ModelSlot {
    Empty,
    Loading,
    Ready,
    Failed,
}

impl ModelSlot {
    fn as_str(self) -> &'static str {
        match self {
            ModelSlot::Empty => "missing",
            ModelSlot::Loading => "loading",
            ModelSlot::Ready => "ready",
            ModelSlot::Failed => "failed",
        }
    }
}

struct AppState {
    config: Config,
    config_path: PathBuf,
    hotkey_handle: Option<hotkey::HotkeyHandle>,
    recording: bool,
    capture_handle: Option<audio::AudioCaptureHandle>,
    whisper: Option<Arc<WhisperContext>>,
    whisper_status: ModelSlot,
    whisper_model_id: String,
    /// Bumped on every load request. A finishing load writes its result only if
    /// its generation is still current, so a superseded load can neither clobber
    /// a newer one nor leave the status wedged.
    whisper_generation: u64,
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
    let needs_model = cfg.provider.mode == "local";
    let model_changed = needs_model && cfg.local.model != s.whisper_model_id;
    s.config = cfg.clone();

    if hotkey_changed {
        s.hotkey_handle.take();
        drop(s);
        let handle = register_hotkey(&cfg.hotkey.record, app_handle.clone());
        let mut s = state.lock().unwrap();
        s.hotkey_handle = handle;
        drop(s);
    } else {
        drop(s);
    }

    if model_changed {
        {
            let mut s = state.lock().unwrap();
            s.whisper = None;
            s.whisper_model_id.clear();
        }
        spawn_model_load(app_handle, cfg.local.model.clone());
    }

    Ok(())
}

#[tauri::command]
fn get_config_path(state: tauri::State<'_, Arc<Mutex<AppState>>>) -> String {
    state.lock().unwrap().config_path.display().to_string()
}

#[tauri::command]
fn list_models() -> Vec<models::ModelInfo> {
    models::list()
}

#[tauri::command]
async fn download_model(id: String, app_handle: AppHandle) -> Result<(), String> {
    models::download(app_handle.clone(), id.clone()).await?;

    let state = app_handle.state::<Arc<Mutex<AppState>>>();
    let should_load = {
        let s = state.lock().unwrap();
        s.config.provider.mode == "local" && s.config.local.model == id
    };
    if should_load {
        spawn_model_load(app_handle.clone(), id);
    }
    Ok(())
}

#[tauri::command]
fn delete_model(id: String, state: tauri::State<'_, Arc<Mutex<AppState>>>) -> Result<(), String> {
    models::delete(&id)?;

    let mut s = state.lock().unwrap();
    if s.whisper_model_id == id || s.whisper_status == ModelSlot::Loading {
        // Bumping invalidates any in-flight load, so deleting a model that is
        // mid-load can't be undone by that load completing afterwards.
        s.whisper_generation += 1;
        s.whisper = None;
        s.whisper_status = ModelSlot::Empty;
        s.whisper_model_id.clear();
    }
    Ok(())
}

#[tauri::command]
fn get_model_status(state: tauri::State<'_, Arc<Mutex<AppState>>>) -> String {
    state.lock().unwrap().whisper_status.as_str().into()
}

/// Loads the whisper context off the caller's thread. First-ever Metal
/// shader-library init costs several seconds, so this must never run on the
/// setup thread or app launch stalls behind it.
fn spawn_model_load(app_handle: AppHandle, model_id: String) {
    let Some(path) = models::model_path(&model_id) else {
        log::error!("[whisper] unknown model id: {}", model_id);
        return;
    };
    if !path.is_file() {
        log::info!("[whisper] model {} not downloaded yet", model_id);
        {
            let state = app_handle.state::<Arc<Mutex<AppState>>>();
            let mut s = state.lock().unwrap();
            // Bump so any in-flight load of a previous model is discarded
            // rather than reporting Ready over this Empty state.
            s.whisper_generation += 1;
            s.whisper = None;
            s.whisper_model_id.clear();
            s.whisper_status = ModelSlot::Empty;
        }
        let _ = app_handle.emit("voicebox:model_status", ModelSlot::Empty.as_str());
        return;
    }

    let generation = {
        let state = app_handle.state::<Arc<Mutex<AppState>>>();
        let mut s = state.lock().unwrap();
        s.whisper_generation += 1;
        s.whisper_status = ModelSlot::Loading;
        s.whisper_generation
    };
    let _ = app_handle.emit("voicebox:model_status", ModelSlot::Loading.as_str());

    std::thread::spawn(move || {
        let start = std::time::Instant::now();
        // whisper.cpp is C++ and can abort on a corrupt model. Without this the
        // status would stay Loading forever and the hotkey would report
        // "loading, try again" with no way out short of restarting.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            WhisperContext::new_with_params(&path, WhisperContextParameters::default())
        }));

        let state = app_handle.state::<Arc<Mutex<AppState>>>();
        let mut s = state.lock().unwrap();

        if s.whisper_generation != generation {
            log::info!("[whisper] discarding superseded load of {}", model_id);
            return;
        }

        let status = match result {
            Ok(Ok(ctx)) => {
                log::info!(
                    "[whisper] loaded {} in {}ms",
                    model_id,
                    start.elapsed().as_millis()
                );
                s.whisper = Some(Arc::new(ctx));
                s.whisper_model_id = model_id;
                ModelSlot::Ready
            }
            Ok(Err(e)) => {
                log::error!("[whisper] failed to load {}: {}", model_id, e);
                s.whisper = None;
                ModelSlot::Failed
            }
            Err(_) => {
                log::error!("[whisper] panicked while loading {}", model_id);
                s.whisper = None;
                ModelSlot::Failed
            }
        };

        s.whisper_status = status;
        drop(s);
        let _ = app_handle.emit("voicebox:model_status", status.as_str());
    });
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
        Some(app_handle.clone()),
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
    let app_stage = app.clone();
    let app_partial = app.clone();
    let on_stage = move |stage: &str| {
        let _ = app_stage.emit(
            "voicebox:state",
            serde_json::json!({
                "state": "processing",
                "stage": stage,
            }),
        );
    };
    let on_partial = move |text: &str| {
        let _ = app_partial.emit("voicebox:partial", text);
    };

    let params = pipeline::AudioParams {
        sample_rate: config.audio.sample_rate,
        channels: config.audio.channels,
        encoding: "pcm_s16le".into(),
    };
    let focus = pipeline::FocusContext::from(&focus_ctx);

    let result = if config.provider.mode == "local" {
        // Bridges the Option: the warm slot may be empty because the model is
        // still loading, was never downloaded, or failed. local_engine::run
        // only ever sees a resolved context.
        let ctx = {
            let state = app.state::<Arc<Mutex<AppState>>>();
            let s = state.lock().unwrap();
            match s.whisper.clone() {
                Some(ctx) => Ok(ctx),
                None => Err(match s.whisper_status {
                    ModelSlot::Loading => "Loading speech model, try again in a moment".to_string(),
                    ModelSlot::Failed => {
                        "Speech model failed to load — try re-downloading it in Settings".to_string()
                    }
                    ModelSlot::Empty | ModelSlot::Ready => {
                        "No speech model downloaded — open Settings to get one".to_string()
                    }
                }),
            }
        };

        match ctx {
            Ok(ctx) => {
                local_engine::run(
                    ctx,
                    &config.local,
                    params,
                    focus,
                    chunk_rx,
                    on_stage,
                    on_partial,
                )
                .await
            }
            Err(message) => Err(message),
        }
    } else {
        pipeline::run(
            &config.cloud.worker_url,
            &config.cloud.token,
            params,
            focus,
            config.beta.streaming_stt,
            chunk_rx,
            on_stage,
            on_partial,
        )
        .await
    };

    match result {
        Ok(result) if result.formatted.trim().is_empty() => {
            // Silence or a stray hotkey tap. Pasting an empty string would
            // clobber the user's current selection.
            log::info!("Empty transcription, nothing to paste");
            hide_overlay(&app);
            let _ = app.emit("voicebox:state", serde_json::json!({"state": "idle"}));
        }
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

/// Headless transcription of a WAV file through the real local pipeline.
/// Exists so local mode can be exercised without a microphone or a hotkey:
///
///     voicebox --transcribe-file path/to/audio.wav
///
/// Uses the same config, model, and `local_engine::run` the hotkey path uses.
fn run_headless_transcribe(cfg: &Config, wav_path: &str) {
    let Some(model_path) = models::model_path(&cfg.local.model) else {
        eprintln!("Unknown model in config: {}", cfg.local.model);
        std::process::exit(2);
    };
    if !model_path.is_file() {
        eprintln!(
            "Model {} not downloaded. Expected at {}",
            cfg.local.model,
            model_path.display()
        );
        std::process::exit(2);
    }

    let pcm = match read_wav_pcm(wav_path) {
        Ok(pcm) => pcm,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(2);
        }
    };

    eprintln!("model:    {} ({})", cfg.local.model, model_path.display());
    eprintln!("input:    {} ({} PCM bytes)", wav_path, pcm.len());
    eprintln!("formatter: {}", cfg.local.formatter);

    let load_start = std::time::Instant::now();
    let ctx = match WhisperContext::new_with_params(&model_path, WhisperContextParameters::default())
    {
        Ok(ctx) => Arc::new(ctx),
        Err(e) => {
            eprintln!("Failed to load model: {}", e);
            std::process::exit(1);
        }
    };
    eprintln!("load:     {}ms", load_start.elapsed().as_millis());

    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let total = std::time::Instant::now();
    let result = runtime.block_on(async {
        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let feeder = tokio::spawn(async move {
            for chunk in pcm.chunks(4096) {
                if tx.send(chunk.to_vec()).await.is_err() {
                    return;
                }
            }
        });

        let out = local_engine::run(
            ctx,
            &cfg.local,
            pipeline::AudioParams {
                sample_rate: cfg.audio.sample_rate,
                channels: cfg.audio.channels,
                encoding: "pcm_s16le".into(),
            },
            pipeline::FocusContext::default(),
            rx,
            |stage| eprintln!("stage:    {}", stage),
            |_| {},
        )
        .await;
        let _ = feeder.await;
        out
    });

    match result {
        Ok(res) if res.formatted.trim().is_empty() => {
            eprintln!("total:    {}ms", total.elapsed().as_millis());
            eprintln!("result:   (empty - silence or clip too short)");
            std::process::exit(3);
        }
        Ok(res) => {
            eprintln!("total:    {}ms", total.elapsed().as_millis());
            eprintln!("---");
            println!("{}", res.formatted);
        }
        Err(e) => {
            eprintln!("Transcription failed: {}", e);
            std::process::exit(1);
        }
    }
}

/// Records from the default input device for `secs` and runs the result
/// through the real pipeline. Exercises `audio::start_capture` (device
/// selection, resampling, chunking) without needing the hotkey:
///
///     voicebox --record-test 6
fn run_headless_record(cfg: &Config, secs: u64) {
    let Some(model_path) = models::model_path(&cfg.local.model) else {
        eprintln!("Unknown model in config: {}", cfg.local.model);
        std::process::exit(2);
    };

    let load_start = std::time::Instant::now();
    let ctx = match WhisperContext::new_with_params(&model_path, WhisperContextParameters::default())
    {
        Ok(ctx) => Arc::new(ctx),
        Err(e) => {
            eprintln!("Failed to load model: {}", e);
            std::process::exit(1);
        }
    };
    eprintln!("model:    {} (loaded in {}ms)", cfg.local.model, load_start.elapsed().as_millis());

    let (handle, chunk_rx) = match audio::start_capture(
        cfg.audio.sample_rate,
        cfg.audio.channels,
        cfg.audio.chunk_size,
        None,
    ) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to start capture: {}", e);
            eprintln!("(microphone permission may be required for the launching terminal)");
            std::process::exit(1);
        }
    };

    eprintln!(">>> RECORDING for {}s - speak now <<<", secs);
    std::thread::sleep(std::time::Duration::from_secs(secs));
    handle.stop();
    drop(handle);
    eprintln!(">>> recording stopped <<<");

    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let result = runtime.block_on(local_engine::run(
        ctx,
        &cfg.local,
        pipeline::AudioParams {
            sample_rate: cfg.audio.sample_rate,
            channels: cfg.audio.channels,
            encoding: "pcm_s16le".into(),
        },
        pipeline::FocusContext::default(),
        chunk_rx,
        |stage| eprintln!("stage:    {}", stage),
        |_| {},
    ));

    match result {
        Ok(res) if res.formatted.trim().is_empty() => {
            eprintln!("result:   (empty - nothing audible was captured)");
            std::process::exit(3);
        }
        Ok(res) => {
            eprintln!("---");
            println!("{}", res.formatted);
        }
        Err(e) => {
            eprintln!("Transcription failed: {}", e);
            std::process::exit(1);
        }
    }
}

fn read_wav_pcm(path: &str) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Reading {}: {}", path, e))?;
    let pos = bytes
        .windows(4)
        .position(|w| w == b"data")
        .ok_or_else(|| format!("{} is not a WAV file (no data chunk)", path))?;
    Ok(bytes[pos + 8..].to_vec())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_log();

    let (cfg, cfg_path) = config::load();

    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--transcribe-file") {
        match args.get(i + 1) {
            Some(path) => return run_headless_transcribe(&cfg, path),
            None => {
                eprintln!("--transcribe-file requires a path");
                std::process::exit(2);
            }
        }
    }
    if let Some(i) = args.iter().position(|a| a == "--record-test") {
        let secs = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(5);
        return run_headless_record(&cfg, secs);
    }

    log::info!("VoiceBox ready (hotkey: {})", cfg.hotkey.record);

    let state = Arc::new(Mutex::new(AppState {
        config: cfg.clone(),
        config_path: cfg_path,
        hotkey_handle: None,
        recording: false,
        capture_handle: None,
        whisper: None,
        whisper_status: ModelSlot::Empty,
        whisper_model_id: String::new(),
        whisper_generation: 0,
    }));

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(state.clone())
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            get_config_path,
            list_models,
            download_model,
            delete_model,
            get_model_status,
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
            let hotkey_handle = register_hotkey(&cfg.hotkey.record, app_handle.clone());
            {
                let mut s = state.lock().unwrap();
                s.hotkey_handle = hotkey_handle;
            }

            // Warm the model in the background. Never inline: first-ever Metal
            // shader init takes seconds and would stall app launch.
            if cfg.provider.mode == "local" {
                spawn_model_load(app_handle, cfg.local.model.clone());
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
