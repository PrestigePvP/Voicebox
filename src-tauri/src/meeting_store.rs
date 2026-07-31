use crate::config::Config;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub start: f64,
    pub end: f64,
    pub speaker: u32,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeakerInfo {
    pub name: Option<String>,
    #[serde(default)]
    pub inferred: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingDoc {
    pub version: u32,
    pub id: String,
    pub created_at_ms: u64,
    #[serde(default)]
    pub duration_sec: f64,
    pub audio_file: String,
    pub status: String, // "recorded" | "complete" | "error"
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub stt_model: Option<String>,
    #[serde(default)]
    pub enrich_model: Option<String>,
    #[serde(default)]
    pub speakers: BTreeMap<String, SpeakerInfo>,
    #[serde(default)]
    pub turns: Vec<Turn>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingSummary {
    pub id: String,
    pub title: Option<String>,
    pub created_at_ms: u64,
    pub duration_sec: f64,
    pub status: String,
}

pub fn meetings_dir(cfg: &Config) -> PathBuf {
    if !cfg.meeting.save_dir.trim().is_empty() {
        return PathBuf::from(cfg.meeting.save_dir.trim());
    }
    dirs::home_dir()
        .map(|h| h.join("Documents").join("VoiceBox Meetings"))
        .unwrap_or_else(|| PathBuf::from("VoiceBox Meetings"))
}

pub fn meeting_dir(cfg: &Config, id: &str) -> PathBuf {
    meetings_dir(cfg).join(id)
}

fn local_timestamp_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    let secs = now.as_secs() as libc::time_t;
    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    unsafe { libc::localtime_r(&secs, &mut tm) };
    format!(
        "{:04}-{:02}-{:02}_{:02}-{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min
    )
}

pub fn create_meeting_dir(cfg: &Config) -> Result<(String, PathBuf), String> {
    let base = meetings_dir(cfg);
    fs::create_dir_all(&base).map_err(|e| format!("Cannot create {}: {}", base.display(), e))?;

    let stem = local_timestamp_id();
    let mut id = stem.clone();
    let mut n = 2;
    while base.join(&id).exists() {
        id = format!("{}-{}", stem, n);
        n += 1;
    }

    let dir = base.join(&id);
    fs::create_dir(&dir).map_err(|e| format!("Cannot create {}: {}", dir.display(), e))?;
    Ok((id, dir))
}

pub fn new_doc(id: &str, created_at_ms: u64) -> MeetingDoc {
    MeetingDoc {
        version: 1,
        id: id.to_string(),
        created_at_ms,
        duration_sec: 0.0,
        audio_file: "audio.wav".into(),
        status: "recorded".into(),
        error: None,
        title: None,
        summary: None,
        stt_model: None,
        enrich_model: None,
        speakers: BTreeMap::new(),
        turns: Vec::new(),
    }
}

pub fn save_doc(dir: &Path, doc: &MeetingDoc) -> Result<(), String> {
    let json = serde_json::to_string_pretty(doc).map_err(|e| e.to_string())?;
    fs::write(dir.join("meeting.json"), json).map_err(|e| e.to_string())?;
    fs::write(dir.join("meeting.md"), render_markdown(doc)).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load_doc(dir: &Path) -> Result<MeetingDoc, String> {
    let path = dir.join("meeting.json");
    let data = fs::read_to_string(&path).map_err(|e| format!("{}: {}", path.display(), e))?;
    serde_json::from_str(&data).map_err(|e| format!("{}: {}", path.display(), e))
}

pub fn list_meetings(cfg: &Config) -> Vec<MeetingSummary> {
    let base = meetings_dir(cfg);
    let Ok(entries) = fs::read_dir(&base) else {
        return Vec::new();
    };

    let mut out: Vec<MeetingSummary> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| load_doc(&e.path()).ok())
        .map(|doc| MeetingSummary {
            id: doc.id,
            title: doc.title,
            created_at_ms: doc.created_at_ms,
            duration_sec: doc.duration_sec,
            status: doc.status,
        })
        .collect();

    out.sort_by(|a, b| b.created_at_ms.cmp(&a.created_at_ms));
    out
}

pub fn display_name(doc: &MeetingDoc, speaker: u32) -> String {
    doc.speakers
        .get(&speaker.to_string())
        .and_then(|s| s.name.clone())
        .unwrap_or_else(|| format!("Speaker {}", speaker + 1))
}

fn format_clock(sec: f64) -> String {
    let s = sec.max(0.0) as u64;
    let h = s / 3600;
    let m = (s % 3600) / 60;
    let r = s % 60;
    if h > 0 {
        format!("{}:{:02}:{:02}", h, m, r)
    } else {
        format!("{}:{:02}", m, r)
    }
}

fn format_date(ms: u64) -> String {
    let secs = (ms / 1000) as libc::time_t;
    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    unsafe { libc::localtime_r(&secs, &mut tm) };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min
    )
}

fn format_duration(sec: f64) -> String {
    let s = sec.max(0.0).round() as u64;
    let h = s / 3600;
    let m = (s % 3600) / 60;
    if h > 0 {
        format!("{}h {:02}m", h, m)
    } else if s >= 60 {
        format!("{}m", m)
    } else {
        format!("{}s", s)
    }
}

pub fn render_markdown(doc: &MeetingDoc) -> String {
    let title = doc
        .title
        .clone()
        .unwrap_or_else(|| format!("Meeting {}", format_date(doc.created_at_ms)));

    let mut md = format!("# {}\n\n", title);
    md.push_str(&format!(
        "{} · {} · {} speakers\n\n",
        format_date(doc.created_at_ms),
        format_duration(doc.duration_sec),
        doc.speakers.len()
    ));

    if let Some(summary) = &doc.summary {
        md.push_str(summary);
        md.push_str("\n\n");
    }

    md.push_str("---\n\n");

    for turn in &doc.turns {
        md.push_str(&format!(
            "**{}** ({}): {}\n\n",
            display_name(doc, turn.speaker),
            format_clock(turn.start),
            turn.text
        ));
    }

    md
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_doc() -> MeetingDoc {
        let mut doc = new_doc("2026-07-30_14-32", 1753886400000);
        doc.duration_sec = 3661.0;
        doc.status = "complete".into();
        doc.title = Some("Q3 Roadmap".into());
        doc.summary = Some("They planned the release.".into());
        doc.speakers.insert(
            "0".into(),
            SpeakerInfo {
                name: Some("Mark".into()),
                inferred: true,
            },
        );
        doc.speakers.insert(
            "1".into(),
            SpeakerInfo {
                name: None,
                inferred: false,
            },
        );
        doc.turns = vec![
            Turn {
                start: 0.3,
                end: 4.0,
                speaker: 0,
                text: "Let's get started.".into(),
            },
            Turn {
                start: 4.5,
                end: 8.0,
                speaker: 1,
                text: "Sounds good.".into(),
            },
        ];
        doc
    }

    #[test]
    fn markdown_renders_names_and_fallback_labels() {
        let md = render_markdown(&sample_doc());
        assert!(md.starts_with("# Q3 Roadmap\n"));
        assert!(md.contains("1h 01m"));
        assert!(md.contains("2 speakers"));
        assert!(md.contains("**Mark** (0:00): Let's get started."));
        assert!(md.contains("**Speaker 2** (0:04): Sounds good."));
        assert!(md.contains("They planned the release."));
    }

    #[test]
    fn doc_roundtrips_through_json() {
        let doc = sample_doc();
        let json = serde_json::to_string(&doc).unwrap();
        assert!(json.contains("\"createdAtMs\""), "camelCase keys expected");
        let back: MeetingDoc = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, doc.id);
        assert_eq!(back.turns.len(), 2);
        assert_eq!(back.speakers.get("0").unwrap().name.as_deref(), Some("Mark"));
        assert!(back.speakers.get("0").unwrap().inferred);
    }

    #[test]
    fn minimal_doc_deserializes_with_defaults() {
        let json = r#"{
            "version": 1, "id": "x", "createdAtMs": 1,
            "audioFile": "audio.wav", "status": "recorded"
        }"#;
        let doc: MeetingDoc = serde_json::from_str(json).unwrap();
        assert_eq!(doc.turns.len(), 0);
        assert_eq!(doc.duration_sec, 0.0);
        assert!(doc.title.is_none());
    }
}
