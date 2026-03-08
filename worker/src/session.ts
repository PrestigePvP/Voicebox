import { DurableObject } from "cloudflare:workers";
import { buildSystemPrompt, buildUserMessage } from "./prompt";
import type { AudioConfig, ClientMessage, FocusContext, ServerMessage } from "./types";
import { wrapPcmAsWav } from "./wav";

const MAX_AUDIO_BYTES = 25 * 1024 * 1024;

const DEFAULT_AUDIO_CONFIG: AudioConfig = { sampleRate: 16000, channels: 1, encoding: "pcm_s16le" };
const DEFAULT_FOCUS_CONTEXT: FocusContext = {
  appName: "",
  bundleID: "",
  elementRole: "",
  title: "",
  placeholder: "",
  value: "",
};

const sendMessage = (ws: WebSocket, msg: ServerMessage) => {
  ws.send(JSON.stringify(msg));
};

const extractText = (
  result: Ai_Cf_Qwen_Qwen3_30B_A3B_Fp8_Chat_Completion_Response | string,
): string | null => {
  if (typeof result === "string") {
    return result;
  }
  const fullContent = result.choices
    ?.map((choice) => choice.message?.content)
    .filter(Boolean)
    .join("\n");
  return fullContent ?? null;
};

export class TranscriptionSession extends DurableObject<Env> {
  private audioChunks: Uint8Array[] = [];
  private totalBytes = 0;
  private audioConfig: AudioConfig = { ...DEFAULT_AUDIO_CONFIG };
  private focusContext: FocusContext = { ...DEFAULT_FOCUS_CONTEXT };

  async fetch(_request: Request): Promise<Response> {
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
        sendMessage(ws, {
          type: "error",
          code: "audio_too_large",
          message: "Audio exceeds 25MB limit",
        });
        ws.close(1009, "Audio too large");
        this.resetState();
        return;
      }

      this.audioChunks.push(chunk);
      return;
    }

    const parsed = JSON.parse(message) as ClientMessage;

    switch (parsed.type) {
      case "configure":
        if (parsed.audio) this.audioConfig = { ...this.audioConfig, ...parsed.audio };
        if (parsed.context) this.focusContext = { ...this.focusContext, ...parsed.context };
        break;

      case "audio_end":
        await this.processAudio(ws);
        break;

      case "cancel":
        this.resetState();
        ws.close(1000, "Cancelled");
        break;
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
    this.focusContext = { ...DEFAULT_FOCUS_CONTEXT };
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
    const wavData = wrapPcmAsWav(
      combined,
      this.audioConfig.sampleRate,
      this.audioConfig.channels,
      bitsPerSample,
    );

    const binaryStr = Array.from(wavData, (byte) => String.fromCharCode(byte)).join("");
    const audioBase64 = btoa(binaryStr);

    let sttResult: { text: string };
    try {
      sttResult = await this.env.AI.run(this.env.STT_MODEL ?? "@cf/openai/whisper-large-v3-turbo", {
        audio: audioBase64,
      });
    } catch (e) {
      const msg = e instanceof Error ? e.message : "STT failed";
      sendMessage(ws, { type: "error", code: "stt_failed", message: msg });
      ws.close(1011, "STT failed");
      this.resetState();
      return;
    }

    sendMessage(ws, { type: "processing", stage: "format" });

    let formatResult: Ai_Cf_Qwen_Qwen3_30B_A3B_Fp8_Chat_Completion_Response | string | null = null;
    try {
      formatResult = await this.env.AI.run(this.env.FORMAT_MODEL ?? "@cf/qwen/qwen3-30b-a3b-fp8", {
        messages: [
          { role: "system", content: buildSystemPrompt(this.focusContext) },
          { role: "user", content: buildUserMessage(sttResult.text, this.focusContext) },
        ],
      }) as Ai_Cf_Qwen_Qwen3_30B_A3B_Fp8_Chat_Completion_Response | string;
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
      formatted: extractText(formatResult) ?? sttResult.text,
    });

    ws.close(1000, "Complete");
    this.resetState();
  }
}
