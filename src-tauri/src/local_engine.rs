use crate::config::LocalConfig;
use crate::pipeline::{AudioParams, FocusContext, PipelineResult};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext};

/// Below this RMS the clip is treated as silence. Whisper invents text from
/// near-silent input, so it never reaches the model.
const SILENCE_RMS: f32 = 0.004;

/// Whisper needs a meaningful window; shorter clips are almost always an
/// accidental hotkey tap.
const MIN_SAMPLES: usize = 16_000 / 5;

pub async fn run<F, G>(
    ctx: Arc<WhisperContext>,
    cfg: &LocalConfig,
    params: AudioParams,
    focus: FocusContext,
    mut chunk_rx: mpsc::Receiver<Vec<u8>>,
    on_stage: F,
    _on_partial: G,
) -> Result<PipelineResult, String>
where
    F: Fn(&str) + Send + 'static,
    G: Fn(&str) + Send + 'static,
{
    let drain_start = Instant::now();
    let mut pcm: Vec<u8> = Vec::new();
    while let Some(chunk) = chunk_rx.recv().await {
        pcm.extend_from_slice(&chunk);
    }
    log::info!(
        "[local] drained {} bytes in {}ms",
        pcm.len(),
        drain_start.elapsed().as_millis()
    );

    let samples = to_mono_f32(&pcm, params.channels);

    if samples.len() < MIN_SAMPLES {
        log::info!("[local] clip too short ({} samples), skipping", samples.len());
        return Ok(PipelineResult::default());
    }
    let level = rms(&samples);
    if level < SILENCE_RMS {
        log::info!("[local] silence (rms {:.5}), skipping", level);
        return Ok(PipelineResult::default());
    }

    on_stage("stt");

    let language = cfg.language.clone();
    let stt_start = Instant::now();
    let audio_secs = samples.len() as f64 / 16_000.0;

    let raw = tokio::task::spawn_blocking(move || {
        catch_unwind(AssertUnwindSafe(|| transcribe(&ctx, &samples, &language)))
            .map_err(|_| "Whisper panicked during transcription".to_string())?
    })
    .await
    .map_err(|e| format!("Transcription task failed: {}", e))??;

    let elapsed = stt_start.elapsed();
    log::info!(
        "[local] stt: {}ms for {:.1}s audio ({:.1}x realtime)",
        elapsed.as_millis(),
        audio_secs,
        audio_secs / elapsed.as_secs_f64().max(0.001)
    );

    let raw = clean(&raw);
    if raw.is_empty() {
        return Ok(PipelineResult::default());
    }

    if cfg.formatter != "ollama" {
        return Ok(PipelineResult {
            formatted: raw.clone(),
            raw,
        });
    }

    on_stage("format");
    let format_start = Instant::now();
    match format_via_ollama(cfg, &raw, &focus).await {
        Ok(formatted) => {
            log::info!("[local] format: {}ms", format_start.elapsed().as_millis());
            Ok(PipelineResult { raw, formatted })
        }
        Err(e) => {
            // A formatter outage must not cost the user their transcript.
            log::warn!("[local] formatter failed, using raw text: {}", e);
            Ok(PipelineResult {
                formatted: raw.clone(),
                raw,
            })
        }
    }
}

fn transcribe(
    ctx: &WhisperContext,
    samples: &[f32],
    language: &str,
) -> Result<String, String> {
    let mut state = ctx
        .create_state()
        .map_err(|e| format!("Creating whisper state: {}", e))?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    if language != "auto" {
        params.set_language(Some(language));
    }
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_suppress_blank(true);

    state
        .full(params, samples)
        .map_err(|e| format!("Whisper inference: {}", e))?;

    Ok(state
        .as_iter()
        .map(|segment| segment.to_string())
        .collect::<Vec<_>>()
        .join(""))
}

fn to_mono_f32(pcm: &[u8], channels: u16) -> Vec<f32> {
    let mut samples: Vec<f32> = pcm
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
        .collect();

    if channels > 1 {
        let n = channels as usize;
        samples = samples
            .chunks(n)
            .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
            .collect();
    }
    samples
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().map(|s| s * s).sum();
    (sum / samples.len() as f32).sqrt()
}

/// Non-speech markers whisper emits for silence, music, and background noise.
const NOISE_MARKERS: &[&str] = &[
    "blank_audio",
    "silence",
    "music",
    "applause",
    "laughter",
    "inaudible",
    "no speech",
    "sound effects",
];

/// Boilerplate leaked from the subtitle corpora whisper was trained on. These
/// are safe to drop outright — unlike phrases such as "Thank you.", which
/// whisper also hallucinates but a user may genuinely dictate. The silence gate
/// above is what prevents those, not a blocklist.
const CREDIT_MARKERS: &[&str] = &[
    "amara.org",
    "subtitles by",
    "subtitled by",
    "transcribed by",
    "subs by",
    "thanks for watching",
];

fn clean(text: &str) -> String {
    let without_markers = strip_bracketed_noise(text);

    let lower = without_markers.to_lowercase();
    if CREDIT_MARKERS.iter().any(|m| lower.contains(m)) {
        log::info!("[local] dropped subtitle-credit boilerplate: {:?}", without_markers);
        return String::new();
    }

    without_markers
}

fn strip_bracketed_noise(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0usize;
    let mut buf = String::new();

    for ch in text.chars() {
        match ch {
            '[' | '(' => {
                depth += 1;
                buf.clear();
            }
            ']' | ')' if depth > 0 => {
                depth -= 1;
                let inner = buf.trim().to_lowercase();
                let is_noise = NOISE_MARKERS
                    .iter()
                    .any(|m| inner.contains(m) || inner.is_empty());
                if !is_noise {
                    out.push(if ch == ']' { '[' } else { '(' });
                    out.push_str(&buf);
                    out.push(ch);
                }
                buf.clear();
            }
            _ if depth > 0 => buf.push(ch),
            _ => out.push(ch),
        }
    }
    out.push_str(&buf);

    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

const SYSTEM_PROMPT: &str = "You are a text formatter. Take the raw speech-to-text transcription and return it with proper punctuation, capitalization, and paragraph breaks. Do not change the words, only fix formatting. Return only the formatted text.";

const CONTEXT_RULES: &str = r#"<rules>
- Chat/messaging apps (Slack, Discord, Messages): casual tone, minimal punctuation, no greeting/signature
- Email apps (Mail, Outlook): proper sentences, appropriate formality
- Code editors (VS Code, Xcode): preserve technical terms and code references exactly, this is often used when chatting to an AI assistant about code, so formatting should be clear and precise
- Search fields: concise, no punctuation unless necessary
- General text fields: standard formatting
- If existing text is present, format the new text so it flows naturally as a continuation
</rules>"#;

fn build_system_prompt(focus: &FocusContext) -> String {
    let has_context = !focus.app_name.is_empty()
        || !focus.element_role.is_empty()
        || !focus.title.is_empty()
        || !focus.placeholder.is_empty()
        || !focus.value.is_empty();

    if has_context {
        format!("{}\n\n{}", SYSTEM_PROMPT, CONTEXT_RULES)
    } else {
        SYSTEM_PROMPT.into()
    }
}

fn build_user_message(transcription: &str, focus: &FocusContext) -> String {
    let fields = [
        ("appName", &focus.app_name),
        ("bundleID", &focus.bundle_id),
        ("elementRole", &focus.element_role),
        ("title", &focus.title),
        ("placeholder", &focus.placeholder),
        ("value", &focus.value),
    ];

    let entries: Vec<String> = fields
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("  <{}>{}</{}>", k, v, k))
        .collect();

    let transcription_entry = format!("  <transcription>{}</transcription>", transcription);

    if entries.is_empty() {
        transcription_entry
    } else {
        format!(
            "<context>{}</context>\n\n{}",
            entries.join("\n"),
            transcription_entry
        )
    }
}

fn strip_think_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<think>") {
        out.push_str(&rest[..start]);
        match rest[start..].find("</think>") {
            Some(end) => rest = &rest[start + end + "</think>".len()..],
            None => return out.trim().into(),
        }
    }
    out.push_str(rest);
    out.trim().into()
}

async fn format_via_ollama(
    cfg: &LocalConfig,
    transcription: &str,
    focus: &FocusContext,
) -> Result<String, String> {
    let url = format!("{}/api/chat", cfg.ollama_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": cfg.ollama_model,
        "messages": [
            { "role": "system", "content": build_system_prompt(focus) },
            { "role": "user", "content": build_user_message(transcription, focus) },
        ],
        "stream": false,
    });

    let response = reqwest::Client::new()
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Ollama request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Ollama returned HTTP {}", response.status()));
    }

    let parsed: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Parsing Ollama response: {}", e))?;

    let content = parsed
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .ok_or("Ollama response missing message.content")?;

    let cleaned = strip_think_tags(content);
    if cleaned.is_empty() {
        return Err("Ollama returned empty content".into());
    }
    Ok(cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_blank_audio_marker() {
        assert_eq!(clean("[BLANK_AUDIO]"), "");
        assert_eq!(clean("[_BLANK_AUDIO_]"), "");
        assert_eq!(clean("(silence)"), "");
        assert_eq!(clean("[Music]"), "");
    }

    #[test]
    fn drops_subtitle_credits() {
        assert_eq!(clean("Subtitles by the Amara.org community"), "");
        assert_eq!(clean("Thanks for watching!"), "");
    }

    #[test]
    fn keeps_genuine_speech_that_resembles_hallucination() {
        // "Thank you." is a real thing to dictate. The silence gate, not a
        // blocklist, is what suppresses the hallucinated version.
        assert_eq!(clean("Thank you."), "Thank you.");
        assert_eq!(clean("Thanks, that works."), "Thanks, that works.");
    }

    #[test]
    fn strips_markers_but_keeps_surrounding_speech() {
        assert_eq!(
            clean("[BLANK_AUDIO] Ship the release today."),
            "Ship the release today."
        );
        assert_eq!(clean("Deploy now (applause) and monitor."), "Deploy now and monitor.");
    }

    #[test]
    fn preserves_meaningful_parentheses() {
        assert_eq!(
            clean("Call getUser(id) before rendering."),
            "Call getUser(id) before rendering."
        );
    }

    #[test]
    fn collapses_whitespace() {
        assert_eq!(clean("  hello   there  "), "hello there");
    }

    #[test]
    fn rms_detects_silence_and_speech() {
        assert!(rms(&[0.0; 1000]) < SILENCE_RMS);
        assert!(rms(&[0.3; 1000]) > SILENCE_RMS);
        assert_eq!(rms(&[]), 0.0);
    }

    #[test]
    fn converts_pcm_s16le_to_f32() {
        // 0, max positive, min negative
        let pcm = [0x00, 0x00, 0xff, 0x7f, 0x00, 0x80];
        let out = to_mono_f32(&pcm, 1);
        assert_eq!(out.len(), 3);
        assert!((out[0] - 0.0).abs() < 1e-6);
        assert!((out[1] - 0.999).abs() < 0.01);
        assert!((out[2] + 1.0).abs() < 1e-6);
    }

    #[test]
    fn downmixes_stereo_to_mono() {
        // two frames of (1.0, 0.0) -> 0.5 each
        let pcm = [0xff, 0x7f, 0x00, 0x00, 0xff, 0x7f, 0x00, 0x00];
        let out = to_mono_f32(&pcm, 2);
        assert_eq!(out.len(), 2);
        assert!((out[0] - 0.5).abs() < 0.01);
    }

    #[test]
    fn strips_think_tags_from_formatter_output() {
        assert_eq!(strip_think_tags("<think>hmm</think>Hello."), "Hello.");
        assert_eq!(strip_think_tags("No tags here."), "No tags here.");
        assert_eq!(strip_think_tags("<think>unterminated"), "");
    }

    #[test]
    fn user_message_includes_context_when_present() {
        let focus = FocusContext {
            app_name: "Slack".into(),
            ..Default::default()
        };
        let msg = build_user_message("hello", &focus);
        assert!(msg.contains("<appName>Slack</appName>"));
        assert!(msg.contains("<transcription>hello</transcription>"));
    }

    #[test]
    fn user_message_omits_empty_context() {
        let msg = build_user_message("hello", &FocusContext::default());
        assert!(!msg.contains("<context>"));
        assert!(msg.contains("<transcription>hello</transcription>"));
    }

    fn test_params() -> AudioParams {
        AudioParams {
            sample_rate: 16_000,
            channels: 1,
            encoding: "pcm_s16le".into(),
        }
    }

    /// Feeds chunks exactly the way `audio.rs` does, then drops the sender the
    /// way `on_hotkey_up` does. If the drain in `run` ever fails to terminate,
    /// this hangs instead of returning — which is the failure mode that would
    /// otherwise leave the overlay stuck in `processing` forever.
    async fn run_with_pcm(ctx: Arc<WhisperContext>, pcm: Vec<u8>) -> PipelineResult {
        let (tx, rx) = mpsc::channel(64);
        tokio::spawn(async move {
            for chunk in pcm.chunks(4096) {
                if tx.send(chunk.to_vec()).await.is_err() {
                    return;
                }
            }
            drop(tx);
        });

        let cfg = LocalConfig::default();
        tokio::time::timeout(
            std::time::Duration::from_secs(120),
            run(ctx, &cfg, test_params(), FocusContext::default(), rx, |_| {}, |_| {}),
        )
        .await
        .expect("engine did not finish - the chunk drain likely never terminated")
        .expect("engine returned an error")
    }

    fn load_test_model() -> Option<Arc<WhisperContext>> {
        let path = std::env::var("VOICEBOX_TEST_MODEL").ok()?;
        let ctx = whisper_rs::WhisperContext::new_with_params(
            &path,
            whisper_rs::WhisperContextParameters::default(),
        )
        .expect("load test model");
        Some(Arc::new(ctx))
    }

    fn wav_pcm(path: &str) -> Vec<u8> {
        let bytes = std::fs::read(path).expect("read wav");
        // Skip to the `data` chunk payload rather than assuming a 44-byte header.
        let pos = bytes
            .windows(4)
            .position(|w| w == b"data")
            .expect("wav has a data chunk");
        bytes[pos + 8..].to_vec()
    }

    #[tokio::test]
    async fn transcribes_real_audio_end_to_end() {
        let Some(ctx) = load_test_model() else {
            eprintln!("skipping: set VOICEBOX_TEST_MODEL to a ggml .bin to run");
            return;
        };
        let wav = std::env::var("VOICEBOX_TEST_WAV").expect("set VOICEBOX_TEST_WAV");

        let result = run_with_pcm(ctx, wav_pcm(&wav)).await;

        assert!(!result.formatted.is_empty(), "expected a transcript");
        let lower = result.formatted.to_lowercase();
        assert!(
            lower.contains("country"),
            "unexpected transcript: {:?}",
            result.formatted
        );
        // formatter defaults to "none", so formatted mirrors raw
        assert_eq!(result.raw, result.formatted);
    }

    #[tokio::test]
    async fn silence_produces_no_transcript() {
        let Some(ctx) = load_test_model() else {
            eprintln!("skipping: set VOICEBOX_TEST_MODEL to a ggml .bin to run");
            return;
        };

        // 3 seconds of digital silence.
        let result = run_with_pcm(ctx, vec![0u8; 16_000 * 2 * 3]).await;

        assert!(
            result.formatted.is_empty(),
            "silence must not produce text, got {:?}",
            result.formatted
        );
    }

    #[tokio::test]
    async fn short_clip_is_gated_before_the_model() {
        let Some(ctx) = load_test_model() else {
            eprintln!("skipping: set VOICEBOX_TEST_MODEL to a ggml .bin to run");
            return;
        };

        // 0.1s of audio, under MIN_SAMPLES - a stray hotkey tap.
        let result = run_with_pcm(ctx, vec![0u8; 16_000 * 2 / 10]).await;
        assert!(result.formatted.is_empty());
    }
}
