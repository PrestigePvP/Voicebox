use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineResult {
    pub raw: String,
    pub formatted: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioParams {
    #[serde(rename = "sampleRate")]
    pub sample_rate: u32,
    pub channels: u16,
    pub encoding: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FocusContext {
    #[serde(rename = "appName")]
    pub app_name: String,
    #[serde(rename = "bundleID")]
    pub bundle_id: String,
    #[serde(rename = "elementRole")]
    pub element_role: String,
    pub title: String,
    pub placeholder: String,
    pub value: String,
}

impl From<&crate::accessibility::FocusContext> for FocusContext {
    fn from(ctx: &crate::accessibility::FocusContext) -> Self {
        Self {
            app_name: ctx.app_name.clone(),
            bundle_id: ctx.bundle_id.clone(),
            element_role: ctx.element_role.clone(),
            title: ctx.title.clone(),
            placeholder: ctx.placeholder.clone(),
            value: ctx.value.clone(),
        }
    }
}

#[derive(Deserialize)]
struct ServerMessage {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(default)]
    stage: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    raw: String,
    #[serde(default)]
    formatted: String,
    #[serde(default)]
    code: String,
    #[serde(default)]
    message: String,
}

#[allow(clippy::too_many_arguments)]
pub async fn run<F, G>(
    worker_url: &str,
    token: &str,
    params: AudioParams,
    focus: FocusContext,
    streaming_stt: bool,
    mut chunk_rx: mpsc::Receiver<Vec<u8>>,
    on_stage: F,
    on_partial: G,
) -> Result<PipelineResult, String>
where
    F: Fn(&str) + Send + 'static,
    G: Fn(&str) + Send + 'static,
{
    let ws_url = worker_url
        .replace("https://", "wss://")
        .replace("http://", "ws://")
        .trim_end_matches('/')
        .to_string();
    let ws_url = if ws_url.ends_with("/ws") {
        ws_url
    } else {
        format!("{}/ws", ws_url)
    };

    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let mut request = ws_url
        .into_client_request()
        .map_err(|e| format!("Failed to build request: {}", e))?;
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {}", token).parse().unwrap());

    let connect_start = std::time::Instant::now();
    let (ws_stream, _) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|e| format!("WebSocket connect: {}", e))?;
    log::info!("[pipeline] ws connect: {}ms", connect_start.elapsed().as_millis());

    let (mut write, mut read) = ws_stream.split();

    // Wait for "ready"
    let ready_msg = read
        .next()
        .await
        .ok_or("Connection closed before ready")?
        .map_err(|e| format!("Reading ready: {}", e))?;

    let ready: ServerMessage = match ready_msg {
        Message::Text(t) => serde_json::from_str(&t).map_err(|e| format!("Parse ready: {}", e))?,
        _ => return Err("Expected text message for ready".into()),
    };
    if ready.msg_type != "ready" {
        return Err(format!("Expected ready, got {}", ready.msg_type));
    }

    // Send configure
    let configure = serde_json::json!({
        "type": "configure",
        "audio": params,
        "context": focus,
        "streamingStt": streaming_stt,
    });
    write
        .send(Message::Text(configure.to_string().into()))
        .await
        .map_err(|e| format!("Sending configure: {}", e))?;

    let pipeline_start = std::time::Instant::now();

    // Spawn sender task
    let send_handle = tokio::spawn(async move {
        let mut chunks_sent = 0u32;
        let mut bytes_sent = 0usize;
        while let Some(chunk) = chunk_rx.recv().await {
            bytes_sent += chunk.len();
            chunks_sent += 1;
            if write.send(Message::Binary(chunk.into())).await.is_err() {
                break;
            }
        }
        log::info!("[pipeline] sender done: {} chunks, {} bytes", chunks_sent, bytes_sent);
        let end = serde_json::json!({"type": "audio_end"});
        let _ = write.send(Message::Text(end.to_string().into())).await;
        write
    });

    // Read responses
    loop {
        let msg = read
            .next()
            .await
            .ok_or("Connection closed")?
            .map_err(|e| format!("Reading response: {}", e))?;

        let server_msg: ServerMessage = match msg {
            Message::Text(t) => {
                serde_json::from_str(&t).map_err(|e| format!("Parse response: {}", e))?
            }
            Message::Close(_) => return Err("Server closed connection".into()),
            _ => continue,
        };

        match server_msg.msg_type.as_str() {
            "processing" => {
                log::info!("[pipeline] server: processing/{} at +{}ms", server_msg.stage, pipeline_start.elapsed().as_millis());
                on_stage(&server_msg.stage);
            }
            "partial" => {
                log::info!("[pipeline] server: partial at +{}ms: \"{}\"", pipeline_start.elapsed().as_millis(), &server_msg.text.chars().take(80).collect::<String>());
                on_partial(&server_msg.text);
            }
            "result" => {
                log::info!("[pipeline] server: result at +{}ms", pipeline_start.elapsed().as_millis());
                let _ = send_handle.await;
                return Ok(PipelineResult {
                    raw: server_msg.raw,
                    formatted: server_msg.formatted,
                });
            }
            "error" => {
                let _ = send_handle.await;
                return Err(format!("{}: {}", server_msg.code, server_msg.message));
            }
            _ => {}
        }
    }
}
