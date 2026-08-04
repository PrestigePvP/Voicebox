use crate::config::Config;
use crate::meeting_api::MeetingClient;
use crate::meeting_store::{self, MeetingDoc, MeetingSummary, SpeakerInfo};
use crate::{audio, wav_writer::WavWriter, AppState};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{menu::MenuItem, AppHandle, Emitter, Manager, Wry};

const HEADER_FLUSH_INTERVAL: Duration = Duration::from_secs(5);

pub struct RecordingSession {
    meeting_id: String,
    dir: PathBuf,
    started_at_ms: u64,
    capture: audio::AudioCaptureHandle,
    writer: tauri::async_runtime::JoinHandle<std::io::Result<u64>>,
}

#[derive(Default)]
pub enum Phase {
    #[default]
    Idle,
    Recording(RecordingSession),
    Processing {
        meeting_id: String,
    },
}

#[derive(Default)]
pub struct MeetingState {
    phase: Phase,
    menu_item: Option<MenuItem<Wry>>,
    last_error: Option<(String, String)>, // (meeting_id, message)
}

fn meeting_state(app: &AppHandle) -> tauri::State<'_, Arc<Mutex<MeetingState>>> {
    app.state::<Arc<Mutex<MeetingState>>>()
}

pub fn init(app: &AppHandle, menu_item: MenuItem<Wry>) {
    let state = Arc::new(Mutex::new(MeetingState {
        phase: Phase::Idle,
        menu_item: Some(menu_item),
        last_error: None,
    }));
    app.manage(state);
}

pub fn is_recording(app: &AppHandle) -> bool {
    matches!(meeting_state(app).lock().unwrap().phase, Phase::Recording(_))
}

fn set_tray_label(state: &MeetingState, recording: bool) {
    if let Some(item) = &state.menu_item {
        let label = if recording {
            "Stop Meeting Recording"
        } else {
            "Start Meeting Recording"
        };
        let _ = item.set_text(label);
    }
}

fn state_json(state: &MeetingState) -> serde_json::Value {
    match &state.phase {
        Phase::Idle => match &state.last_error {
            Some((id, message)) => serde_json::json!({
                "state": "error", "meetingId": id, "message": message,
            }),
            None => serde_json::json!({ "state": "idle" }),
        },
        Phase::Recording(session) => serde_json::json!({
            "state": "recording",
            "meetingId": session.meeting_id,
            "startedAtMs": session.started_at_ms,
        }),
        Phase::Processing { meeting_id } => serde_json::json!({
            "state": "transcribing", "meetingId": meeting_id,
        }),
    }
}

fn emit_state(app: &AppHandle, payload: serde_json::Value) {
    let _ = app.emit("voicebox:meeting-state", payload);
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn app_config(app: &AppHandle) -> Config {
    let state = app.state::<Arc<Mutex<AppState>>>();
    let s = state.lock().unwrap();
    s.config.clone()
}

pub fn start(app: &AppHandle) -> Result<(), String> {
    let config = app_config(app);

    {
        let lib_state = app.state::<Arc<Mutex<AppState>>>();
        if lib_state.lock().unwrap().recording {
            return Err("A dictation recording is in progress".into());
        }
    }

    // Fail before recording an hour of audio that can't be processed.
    MeetingClient::new(&config.cloud.worker_url, &config.cloud.token)?;

    let state = meeting_state(app);
    {
        let s = state.lock().unwrap();
        match s.phase {
            Phase::Idle => {}
            Phase::Recording(_) => return Err("Already recording a meeting".into()),
            Phase::Processing { .. } => return Err("Still processing the previous meeting".into()),
        }
    }

    let (meeting_id, dir) = meeting_store::create_meeting_dir(&config)?;
    let started_at_ms = now_millis();

    let doc = meeting_store::new_doc(&meeting_id, started_at_ms);
    meeting_store::save_doc(&dir, &doc)?;

    let (capture, mut chunk_rx) = audio::start_capture(
        config.audio.sample_rate,
        config.audio.channels,
        config.audio.chunk_size,
        app.clone(),
    )?;

    let wav_path = dir.join(&doc.audio_file);
    let sample_rate = config.audio.sample_rate;
    let channels = config.audio.channels;
    let writer = tauri::async_runtime::spawn_blocking(move || -> std::io::Result<u64> {
        let mut writer = WavWriter::create(&wav_path, sample_rate, channels)?;
        let mut last_flush = Instant::now();
        while let Some(chunk) = chunk_rx.blocking_recv() {
            writer.write(&chunk)?;
            if last_flush.elapsed() >= HEADER_FLUSH_INTERVAL {
                writer.flush_header()?;
                last_flush = Instant::now();
            }
        }
        writer.finalize()
    });

    let payload = {
        let mut s = state.lock().unwrap();
        s.last_error = None;
        s.phase = Phase::Recording(RecordingSession {
            meeting_id: meeting_id.clone(),
            dir,
            started_at_ms,
            capture,
            writer,
        });
        set_tray_label(&s, true);
        state_json(&s)
    };

    crate::show_overlay(app);
    emit_state(app, payload);
    log::info!("[meeting] started {}", meeting_id);
    Ok(())
}

pub fn stop(app: &AppHandle) -> Result<(), String> {
    let state = meeting_state(app);
    let session = {
        let mut s = state.lock().unwrap();
        match std::mem::take(&mut s.phase) {
            Phase::Recording(session) => {
                s.phase = Phase::Processing {
                    meeting_id: session.meeting_id.clone(),
                };
                set_tray_label(&s, false);
                session
            }
            other => {
                s.phase = other;
                return Err("No meeting recording in progress".into());
            }
        }
    };

    crate::hide_overlay(app);

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let RecordingSession {
            meeting_id,
            dir,
            started_at_ms: _,
            capture,
            writer,
        } = session;

        // Dropping joins the cpal thread, which closes the chunk channel and
        // lets the writer drain and finalize the WAV.
        let _ = tokio::task::spawn_blocking(move || drop(capture)).await;

        let sample_rate = {
            let state = app.state::<Arc<Mutex<AppState>>>();
            let s = state.lock().unwrap();
            s.config.audio.sample_rate
        };

        match writer.await {
            Ok(Ok(data_bytes)) => {
                let bytes_per_sec = (sample_rate * 2) as f64;
                if let Ok(mut doc) = meeting_store::load_doc(&dir) {
                    doc.duration_sec = data_bytes as f64 / bytes_per_sec;
                    let _ = meeting_store::save_doc(&dir, &doc);
                }
                log::info!("[meeting] {} recorded {} bytes", meeting_id, data_bytes);
                process(app, meeting_id).await;
            }
            Ok(Err(e)) => fail(&app, &meeting_id, format!("Recording write failed: {}", e)),
            Err(e) => fail(&app, &meeting_id, format!("Recording writer crashed: {}", e)),
        }
    });

    Ok(())
}

fn fail(app: &AppHandle, meeting_id: &str, message: String) {
    log::error!("[meeting] {} failed: {}", meeting_id, message);

    let config = app_config(app);
    let dir = meeting_store::meeting_dir(&config, meeting_id);
    if let Ok(mut doc) = meeting_store::load_doc(&dir) {
        doc.status = "error".into();
        doc.error = Some(message.clone());
        let _ = meeting_store::save_doc(&dir, &doc);
    }

    let state = meeting_state(app);
    {
        let mut s = state.lock().unwrap();
        s.phase = Phase::Idle;
        s.last_error = Some((meeting_id.to_string(), message.clone()));
        set_tray_label(&s, false);
    }
    emit_state(
        app,
        serde_json::json!({ "state": "error", "meetingId": meeting_id, "message": message }),
    );
}

pub enum ProcessEvent {
    Uploading(f32),
    Transcribing,
    Formatting,
}

/// The upload → transcribe → enrich → save pipeline, independent of Tauri
/// state and events so the headless `--meeting-file` path can drive it too.
pub async fn run_processing(
    config: &Config,
    meeting_id: &str,
    mut on_event: impl FnMut(ProcessEvent),
) -> Result<MeetingDoc, String> {
    let dir = meeting_store::meeting_dir(config, meeting_id);
    let mut doc = meeting_store::load_doc(&dir)?;
    let client = MeetingClient::new(&config.cloud.worker_url, &config.cloud.token)?;

    on_event(ProcessEvent::Uploading(0.0));
    let key = client
        .upload_wav(&dir.join(&doc.audio_file), |progress| {
            on_event(ProcessEvent::Uploading(progress));
        })
        .await?;

    on_event(ProcessEvent::Transcribing);
    let transcription = client.transcribe(&key).await?;

    on_event(ProcessEvent::Formatting);
    let enrich = client.enrich(&transcription.turns).await;

    let mut speaker_ids: Vec<u32> = transcription.turns.iter().map(|t| t.speaker).collect();
    speaker_ids.sort_unstable();
    speaker_ids.dedup();

    let mut speakers = BTreeMap::new();
    for id in speaker_ids {
        let name = enrich
            .as_ref()
            .and_then(|e| e.speakers.get(&id.to_string()).cloned())
            .flatten();
        speakers.insert(
            id.to_string(),
            SpeakerInfo {
                inferred: name.is_some(),
                name,
            },
        );
    }

    if doc.duration_sec <= 0.0 {
        doc.duration_sec = transcription.duration_sec;
    }
    doc.turns = transcription.turns;
    doc.speakers = speakers;
    doc.stt_model = Some(transcription.model);
    if let Some(e) = &enrich {
        doc.title = e.title.clone();
        doc.summary = e.summary.clone();
        doc.enrich_model = Some(e.model.clone());
    }
    doc.status = "complete".into();
    doc.error = None;

    meeting_store::save_doc(&dir, &doc)
        .map_err(|e| format!("Saving transcript failed: {}", e))?;
    Ok(doc)
}

async fn process(app: AppHandle, meeting_id: String) {
    let config = app_config(&app);

    let app_events = app.clone();
    let event_id = meeting_id.clone();
    let result = run_processing(&config, &meeting_id, move |event| {
        let payload = match event {
            ProcessEvent::Uploading(progress) => serde_json::json!({
                "state": "uploading", "meetingId": event_id, "progress": progress,
            }),
            ProcessEvent::Transcribing => serde_json::json!({
                "state": "transcribing", "meetingId": event_id,
            }),
            ProcessEvent::Formatting => serde_json::json!({
                "state": "formatting", "meetingId": event_id,
            }),
        };
        emit_state(&app_events, payload);
    })
    .await;

    match result {
        Ok(doc) => {
            {
                let state = meeting_state(&app);
                let mut s = state.lock().unwrap();
                s.phase = Phase::Idle;
                s.last_error = None;
            }
            emit_state(
                &app,
                serde_json::json!({ "state": "complete", "meetingId": meeting_id }),
            );
            log::info!("[meeting] {} complete ({} turns)", meeting_id, doc.turns.len());
        }
        Err(e) => fail(&app, &meeting_id, e),
    }
}

// --- IPC commands ---

#[tauri::command]
pub fn start_meeting(app: AppHandle) -> Result<(), String> {
    start(&app)
}

#[tauri::command]
pub fn stop_meeting(app: AppHandle) -> Result<(), String> {
    stop(&app)
}

#[tauri::command]
pub fn get_meeting_state(app: AppHandle) -> serde_json::Value {
    let state = meeting_state(&app);
    let s = state.lock().unwrap();
    state_json(&s)
}

#[tauri::command]
pub fn retry_meeting(app: AppHandle, id: String) -> Result<(), String> {
    let state = meeting_state(&app);
    {
        let mut s = state.lock().unwrap();
        match s.phase {
            Phase::Idle => {}
            _ => return Err("A meeting is already recording or processing".into()),
        }
        s.phase = Phase::Processing {
            meeting_id: id.clone(),
        };
        s.last_error = None;
    }

    let config = app_config(&app);
    let wav = meeting_store::meeting_dir(&config, &id).join("audio.wav");
    if !wav.is_file() {
        let mut s = state.lock().unwrap();
        s.phase = Phase::Idle;
        return Err(format!("No audio file for meeting {}", id));
    }

    tauri::async_runtime::spawn(async move {
        process(app, id).await;
    });
    Ok(())
}

#[tauri::command]
pub fn list_meetings(app: AppHandle) -> Vec<MeetingSummary> {
    meeting_store::list_meetings(&app_config(&app))
}

#[tauri::command]
pub fn load_meeting(app: AppHandle, id: String) -> Result<MeetingDoc, String> {
    meeting_store::load_doc(&meeting_store::meeting_dir(&app_config(&app), &id))
}

#[tauri::command]
pub fn rename_speaker(
    app: AppHandle,
    id: String,
    speaker: String,
    name: String,
) -> Result<MeetingDoc, String> {
    let dir = meeting_store::meeting_dir(&app_config(&app), &id);
    let mut doc = meeting_store::load_doc(&dir)?;

    let entry = doc
        .speakers
        .entry(speaker)
        .or_insert(SpeakerInfo { name: None, inferred: false });
    let trimmed = name.trim();
    entry.name = if trimmed.is_empty() { None } else { Some(trimmed.to_string()) };
    entry.inferred = false;

    meeting_store::save_doc(&dir, &doc)?;
    Ok(doc)
}

#[tauri::command]
pub fn open_meetings_folder(app: AppHandle) -> Result<(), String> {
    let dir = meeting_store::meetings_dir(&app_config(&app));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::process::Command::new("open")
        .arg(&dir)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
