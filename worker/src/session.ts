import { DurableObject } from "cloudflare:workers";
import type { AudioConfig, ClientMessage, Env, ServerMessage } from "./types";
import { wrapPcmAsWav } from "./wav";

const MAX_AUDIO_BYTES = 25 * 1024 * 1024;

const FORMATTING_PROMPT =
  "You are a text formatter. Take the raw speech-to-text transcription and return it with proper punctuation, capitalization, and paragraph breaks. Do not change the words, only fix formatting. Return only the formatted text.";

const sendMessage = (ws: WebSocket, msg: ServerMessage) => {
  ws.send(JSON.stringify(msg));
};

export class TranscriptionSession extends DurableObject<Env> {
  private audioChunks: Uint8Array[] = [];
  private totalBytes = 0;
  private audioConfig: AudioConfig = { sampleRate: 16000, channels: 1, encoding: "pcm_s16le" };

  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);
    const sampleRate = parseInt(
      request.headers.get("X-VoiceBox-Sample-Rate") ?? url.searchParams.get("sampleRate") ?? "16000",
      10,
    );
    const channels = parseInt(
      request.headers.get("X-VoiceBox-Channels") ?? url.searchParams.get("channels") ?? "1",
      10,
    );
    const encoding =
      request.headers.get("X-VoiceBox-Encoding") ?? url.searchParams.get("encoding") ?? "pcm_s16le";
    this.audioConfig = { sampleRate, channels, encoding };

    const pair = new WebSocketPair();
    const [client, server] = [pair[0], pair[1]];

    this.ctx.acceptWebSocket(server);
    sendMessage(server, { type: "ready" });

    return new Response(null, { status: 101, webSocket: client });
  }

  async webSocketMessage(ws: WebSocket, message: string | ArrayBuffer) {
    if (message instanceof ArrayBuffer) {
      const chunk = new Uint8Array(message);
      this.totalBytes += chunk.byteLength;

      if (this.totalBytes > MAX_AUDIO_BYTES) {
        sendMessage(ws, { type: "error", code: "audio_too_large", message: "Audio exceeds 25MB limit" });
        ws.close(1009, "Audio too large");
        this.resetState();
        return;
      }

      this.audioChunks.push(chunk);
      return;
    }

    const parsed = JSON.parse(message) as ClientMessage;

    if (parsed.type === "audio_end") {
      await this.processAudio(ws);
    } else if (parsed.type === "cancel") {
      this.resetState();
      ws.close(1000, "Cancelled");
    }
  }

  async webSocketClose(_ws: WebSocket, _code: number, _reason: string, _wasClean: boolean) {
    this.resetState();
  }

  async webSocketError(_ws: WebSocket, _error: unknown) {
    this.resetState();
  }

  private resetState() {
    this.audioChunks = [];
    this.totalBytes = 0;
  }

  private async processAudio(ws: WebSocket) {
    sendMessage(ws, { type: "processing", stage: "stt" });

    const combined = new Uint8Array(this.totalBytes);
    let offset = 0;
    for (const chunk of this.audioChunks) {
      combined.set(chunk, offset);
      offset += chunk.byteLength;
    }

    const bitsPerSample = 16;
    const wavData = wrapPcmAsWav(combined, this.audioConfig.sampleRate, this.audioConfig.channels, bitsPerSample);

    const binaryStr = Array.from(wavData, (byte) => String.fromCharCode(byte)).join("");
    const audioBase64 = btoa(binaryStr);

    let sttResult: { text: string };
    try {
      sttResult = (await this.env.AI.run("@cf/openai/whisper-large-v3-turbo", {
        audio: audioBase64,
      })) as { text: string };
    } catch (e) {
      const msg = e instanceof Error ? e.message : "STT failed";
      sendMessage(ws, { type: "error", code: "stt_failed", message: msg });
      ws.close(1011, "STT failed");
      this.resetState();
      return;
    }

    sendMessage(ws, { type: "processing", stage: "format" });

    let formatted: { response: string | null };
    try {
      formatted = (await this.env.AI.run("@cf/ibm-granite/granite-4.0-h-micro", {
        messages: [
          { role: "system", content: FORMATTING_PROMPT },
          { role: "user", content: sttResult.text },
        ],
      })) as { response: string | null };
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Formatting failed";
      sendMessage(ws, { type: "error", code: "format_failed", message: msg });
      ws.close(1011, "Format failed");
      this.resetState();
      return;
    }

    sendMessage(ws, {
      type: "result",
      raw: sttResult.text,
      formatted: formatted.response ?? sttResult.text,
    });

    ws.close(1000, "Complete");
    this.resetState();
  }
}
