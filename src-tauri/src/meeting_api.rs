use crate::meeting_store::Turn;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

const PART_SIZE: u64 = 10 * 1024 * 1024; // uniform part size — R2 requires all parts except the last to match
const PART_ATTEMPTS: u32 = 3;
const PART_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const TRANSCRIBE_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const ENRICH_TIMEOUT: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscribeResponse {
    #[serde(default)]
    pub duration_sec: f64,
    pub model: String,
    pub turns: Vec<Turn>,
}

#[derive(Debug, Deserialize)]
pub struct EnrichResponse {
    pub title: Option<String>,
    pub summary: Option<String>,
    #[serde(default)]
    pub speakers: BTreeMap<String, Option<String>>,
    pub model: String,
}

#[derive(Deserialize)]
struct UploadInitResponse {
    key: String,
    #[serde(rename = "uploadId")]
    upload_id: String,
}

#[derive(Deserialize)]
struct UploadPartResponse {
    #[serde(rename = "partNumber")]
    part_number: u32,
    etag: String,
}

#[derive(Deserialize)]
struct ApiErrorBody {
    #[serde(default)]
    error: String,
    #[serde(default)]
    message: String,
}

pub struct MeetingClient {
    http: reqwest::Client,
    base_url: String,
    token: String,
}

/// The dictation config stores a WebSocket URL (wss://…/ws); the meeting API
/// is plain HTTPS on the same worker.
fn http_base_url(worker_url: &str) -> String {
    let mut url = worker_url.trim().to_string();
    if let Some(rest) = url.strip_prefix("wss://") {
        url = format!("https://{}", rest);
    } else if let Some(rest) = url.strip_prefix("ws://") {
        url = format!("http://{}", rest);
    }
    let url = url.trim_end_matches('/');
    let url = url.strip_suffix("/ws").unwrap_or(url);
    url.trim_end_matches('/').to_string()
}

async fn api_error(response: reqwest::Response) -> String {
    let status = response.status();
    match response.json::<ApiErrorBody>().await {
        Ok(body) if !body.message.is_empty() => format!("{} ({})", body.message, body.error),
        Ok(body) => format!("HTTP {} ({})", status, body.error),
        Err(_) => format!("HTTP {}", status),
    }
}

impl MeetingClient {
    pub fn new(worker_url: &str, token: &str) -> Result<Self, String> {
        if worker_url.trim().is_empty() || token.trim().is_empty() {
            return Err(
                "Meeting processing needs the cloud worker URL and token configured in Settings"
                    .into(),
            );
        }
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            http,
            base_url: http_base_url(worker_url),
            token: token.trim().to_string(),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        req.header("Authorization", format!("Bearer {}", self.token))
    }

    pub async fn upload_wav(
        &self,
        wav_path: &Path,
        mut on_progress: impl FnMut(f32),
    ) -> Result<String, String> {
        let total = std::fs::metadata(wav_path)
            .map_err(|e| format!("Cannot read {}: {}", wav_path.display(), e))?
            .len();
        if total == 0 {
            return Err("Audio file is empty".into());
        }

        let init: UploadInitResponse = {
            let response = self
                .auth(self.http.post(self.url("/meetings/uploads")))
                .timeout(Duration::from_secs(60))
                .json(&serde_json::json!({ "sizeBytes": total }))
                .send()
                .await
                .map_err(|e| format!("Upload init failed: {}", e))?;
            if !response.status().is_success() {
                return Err(format!("Upload init failed: {}", api_error(response).await));
            }
            response.json().await.map_err(|e| e.to_string())?
        };

        let mut file = std::fs::File::open(wav_path).map_err(|e| e.to_string())?;
        let part_count = total.div_ceil(PART_SIZE);
        let mut parts: Vec<serde_json::Value> = Vec::with_capacity(part_count as usize);
        let mut uploaded: u64 = 0;

        for part_number in 1..=part_count {
            let len = PART_SIZE.min(total - uploaded);
            let mut buf = vec![0u8; len as usize];
            file.seek(SeekFrom::Start(uploaded)).map_err(|e| e.to_string())?;
            file.read_exact(&mut buf).map_err(|e| e.to_string())?;

            let part = self.upload_part(&init.key, &init.upload_id, part_number as u32, buf).await?;
            parts.push(serde_json::json!({
                "partNumber": part.part_number,
                "etag": part.etag,
            }));

            uploaded += len;
            on_progress(uploaded as f32 / total as f32);
        }

        let response = self
            .auth(self.http.post(self.url("/meetings/uploads/complete")))
            .timeout(Duration::from_secs(60))
            .json(&serde_json::json!({
                "key": init.key,
                "uploadId": init.upload_id,
                "parts": parts,
            }))
            .send()
            .await
            .map_err(|e| format!("Upload complete failed: {}", e))?;
        if !response.status().is_success() {
            return Err(format!("Upload complete failed: {}", api_error(response).await));
        }

        Ok(init.key)
    }

    async fn upload_part(
        &self,
        key: &str,
        upload_id: &str,
        part_number: u32,
        data: Vec<u8>,
    ) -> Result<UploadPartResponse, String> {
        let url = format!(
            "{}?key={}&uploadId={}&partNumber={}",
            self.url("/meetings/uploads/part"),
            urlencode(key),
            urlencode(upload_id),
            part_number
        );

        let mut last_err = String::new();
        for attempt in 0..PART_ATTEMPTS {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_secs(1 << attempt)).await;
            }
            let result = self
                .auth(self.http.put(&url))
                .timeout(PART_TIMEOUT)
                .body(data.clone())
                .send()
                .await;
            match result {
                Ok(response) if response.status().is_success() => {
                    return response.json().await.map_err(|e| e.to_string());
                }
                Ok(response) => last_err = api_error(response).await,
                Err(e) => last_err = e.to_string(),
            }
            log::warn!(
                "[meeting] part {} attempt {} failed: {}",
                part_number,
                attempt + 1,
                last_err
            );
        }
        Err(format!("Uploading part {} failed: {}", part_number, last_err))
    }

    pub async fn transcribe(&self, key: &str) -> Result<TranscribeResponse, String> {
        let response = self
            .auth(self.http.post(self.url("/meetings/transcribe")))
            .timeout(TRANSCRIBE_TIMEOUT)
            .json(&serde_json::json!({ "key": key }))
            .send()
            .await
            .map_err(|e| format!("Transcription request failed: {}", e))?;
        if !response.status().is_success() {
            return Err(format!("Transcription failed: {}", api_error(response).await));
        }
        response.json().await.map_err(|e| format!("Bad transcription response: {}", e))
    }

    /// Best-effort: a failed enrich must never fail the meeting.
    pub async fn enrich(&self, turns: &[Turn]) -> Option<EnrichResponse> {
        let result = self
            .auth(self.http.post(self.url("/meetings/enrich")))
            .timeout(ENRICH_TIMEOUT)
            .json(&serde_json::json!({ "turns": turns }))
            .send()
            .await;

        match result {
            Ok(response) if response.status().is_success() => match response.json().await {
                Ok(enrich) => Some(enrich),
                Err(e) => {
                    log::warn!("[meeting] enrich response parse failed: {}", e);
                    None
                }
            },
            Ok(response) => {
                log::warn!("[meeting] enrich failed: {}", api_error(response).await);
                None
            }
            Err(e) => {
                log::warn!("[meeting] enrich request failed: {}", e);
                None
            }
        }
    }
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{:02X}", b),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_base_url_normalizes_ws_schemes_and_suffix() {
        assert_eq!(
            http_base_url("wss://voicebox.example.workers.dev"),
            "https://voicebox.example.workers.dev"
        );
        assert_eq!(
            http_base_url("wss://voicebox.example.workers.dev/ws"),
            "https://voicebox.example.workers.dev"
        );
        assert_eq!(http_base_url("ws://localhost:8787/ws/"), "http://localhost:8787");
        assert_eq!(
            http_base_url("https://voicebox.example.workers.dev/"),
            "https://voicebox.example.workers.dev"
        );
    }

    #[test]
    fn urlencode_escapes_non_unreserved() {
        assert_eq!(urlencode("m-abc.wav"), "m-abc.wav");
        assert_eq!(urlencode("a b+c"), "a%20b%2Bc");
    }
}
