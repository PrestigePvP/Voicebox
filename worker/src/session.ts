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
  result: { response?: string } | string,
): string | null => {
  if (typeof result === "string") return result;
  return result.response ?? null;
};

export class TranscriptionSession extends DurableObject<Env> {
  private audioChunks: Uint8Array[] = [];
  private totalBytes = 0;
  private audioConfig: AudioConfig = { ...DEFAULT_AUDIO_CONFIG };
  private focusContext: FocusContext = { ...DEFAULT_FOCUS_CONTEXT };
  private streamingStt = false;
  private sttPromise: Promise<void> | null = null;
  private activeWs: WebSocket | null = null;
  private lastPartialText: string | null = null;
  private lastPartialBytes = 0;
  private lastPartialFormatted: string | null = null;
  private checkpointRaw: string | null = null;
  private checkpointFormatted: string | null = null;
  private committedText = "";
  private lastProcessedChunk = 0;
  private prevSegmentStart = 0;

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

      const STT_CHUNK_THRESHOLD = 12;
      if (this.streamingStt && !this.sttPromise
        && this.audioChunks.length - this.lastProcessedChunk >= STT_CHUNK_THRESHOLD) {
        this.sttPromise = this.runPeriodicStt().finally(() => {
          this.sttPromise = null;
        });
      }

      return;
    }

    const parsed = JSON.parse(message) as ClientMessage;

    switch (parsed.type) {
      case "configure":
        if (parsed.audio) this.audioConfig = { ...this.audioConfig, ...parsed.audio };
        if (parsed.context) this.focusContext = { ...this.focusContext, ...parsed.context };
        if (parsed.streamingStt) {
          this.streamingStt = true;
          this.activeWs = ws;
        }
        break;

      case "audio_end": {
        if (this.sttPromise) {
          const t = Date.now();
          console.log(`[audio_end] awaiting in-flight partial STT...`);
          await this.sttPromise;
          console.log(`[audio_end] partial STT await took ${Date.now() - t}ms`);
        }
        await this.processAudio(ws);
        break;
      }

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
    this.sttPromise = null;
    this.activeWs = null;
    this.streamingStt = false;
    this.lastPartialText = null;
    this.lastPartialBytes = 0;
    this.lastPartialFormatted = null;
    this.checkpointRaw = null;
    this.checkpointFormatted = null;
    this.committedText = "";
    this.lastProcessedChunk = 0;
    this.prevSegmentStart = 0;
    this.audioChunks = [];
    this.totalBytes = 0;
    this.focusContext = { ...DEFAULT_FOCUS_CONTEXT };
  }

  private combineChunkRange(start: number, end: number): Uint8Array {
    let total = 0;
    for (let i = start; i < end; i++) total += this.audioChunks[i].byteLength;
    const combined = new Uint8Array(total);
    let offset = 0;
    for (let i = start; i < end; i++) {
      combined.set(this.audioChunks[i], offset);
      offset += this.audioChunks[i].byteLength;
    }
    return combined;
  }

  private async runPeriodicStt() {
    const currentChunkCount = this.audioChunks.length;
    if (currentChunkCount <= this.lastProcessedChunk || !this.activeWs) return;

    // Include previous segment as context for Whisper (helps with word boundaries)
    const contextStart = this.lastProcessedChunk > 0
      ? Math.max(0, this.prevSegmentStart)
      : 0;
    const windowAudio = this.combineChunkRange(contextStart, currentChunkCount);
    const MIN_BYTES = 16000;
    if (windowAudio.byteLength < MIN_BYTES) return;

    const contextBytes = contextStart < this.lastProcessedChunk
      ? this.combineChunkRange(contextStart, this.lastProcessedChunk).byteLength
      : 0;

    const segSecs = (windowAudio.byteLength / (this.audioConfig.sampleRate * 2)).toFixed(1);
    const totalSecs = (this.totalBytes / (this.audioConfig.sampleRate * 2)).toFixed(1);
    console.log(`[periodic-stt] window [${contextStart}..${currentChunkCount}] (context: ${contextBytes} bytes), ${windowAudio.byteLength} bytes (~${segSecs}s), total ~${totalSecs}s`);
    const t0 = Date.now();

    const wavData = wrapPcmAsWav(windowAudio, this.audioConfig.sampleRate, this.audioConfig.channels, 16);
    const binaryStr = Array.from(wavData, (byte) => String.fromCharCode(byte)).join("");
    const audioBase64 = btoa(binaryStr);

    try {
      const sttResult: { text: string } = await this.env.AI.run(
        this.env.STT_MODEL ?? "@cf/openai/whisper-large-v3-turbo",
        { audio: audioBase64 },
      );
      const windowText = sttResult.text.trim();
      console.log(`[periodic-stt] stt done in ${Date.now() - t0}ms: "${windowText.slice(0, 80)}"`);

      // Extract only the new portion using proportional split based on audio duration
      let newText: string;
      if (contextBytes > 0 && windowAudio.byteLength > 0) {
        const contextRatio = contextBytes / windowAudio.byteLength;
        const words = windowText.split(/\s+/).filter(Boolean);
        const skipWords = Math.round(words.length * contextRatio);
        newText = words.slice(skipWords).join(" ");
        console.log(`[periodic-stt] context ratio ${(contextRatio * 100).toFixed(0)}%, skip ${skipWords}/${words.length} words, new: "${newText.slice(0, 60)}"`);
      } else {
        newText = windowText;
      }

      if (this.committedText && newText) {
        this.committedText += " " + newText;
      } else if (newText) {
        this.committedText = newText;
      }

      this.prevSegmentStart = this.lastProcessedChunk;
      this.lastProcessedChunk = currentChunkCount;
      this.lastPartialText = this.committedText;
      this.lastPartialBytes = this.totalBytes;
      sendMessage(this.activeWs, { type: "partial", text: this.committedText });

      const fmtStart = Date.now();
      const formatResult = await this.env.AI.run(
        this.env.FORMAT_MODEL ?? "@cf/meta/llama-3.2-3b-instruct",
        {
          messages: [
            { role: "system", content: buildSystemPrompt(this.focusContext) },
            { role: "user", content: buildUserMessage(this.committedText, this.focusContext) },
          ],
          temperature: 0,
        },
      );
      const formatted = extractText(formatResult) ?? this.committedText;
      this.lastPartialFormatted = formatted;
      console.log(`[periodic-stt] format done in ${Date.now() - fmtStart}ms, total cycle ${Date.now() - t0}ms`);

      const boundaryMatch = formatted.match(/.*[.!?]/s);
      if (boundaryMatch) {
        this.checkpointRaw = this.committedText;
        this.checkpointFormatted = boundaryMatch[0];
        console.log(`[periodic-stt] checkpoint at ${this.checkpointFormatted.length}/${formatted.length} chars`);
      }
    } catch (e) {
      console.log(`[periodic-stt] failed in ${Date.now() - t0}ms: ${e}`);
      this.lastProcessedChunk = currentChunkCount;
    }
  }

  private async processAudio(ws: WebSocket) {
    const t0 = Date.now();
    const audioSeconds = (this.totalBytes / (this.audioConfig.sampleRate * 2)).toFixed(1);
    console.log(`[processAudio] start, ${this.totalBytes} bytes (~${audioSeconds}s audio)`);

    let sttText: string;

    if (this.lastPartialText && this.streamingStt) {
      const TAIL_MIN_BYTES = 8000;
      const hasUnprocessed = this.audioChunks.length > this.lastProcessedChunk;
      const tailAudio = hasUnprocessed
        ? this.combineChunkRange(this.lastProcessedChunk, this.audioChunks.length)
        : null;
      const hasTail = tailAudio && tailAudio.byteLength >= TAIL_MIN_BYTES;

      if (!hasTail && this.lastPartialFormatted) {
        console.log(`[processAudio] fast path, reusing partial+formatted`);
        console.log(`[processAudio] total: ${Date.now() - t0}ms`);
        sendMessage(ws, {
          type: "result",
          raw: this.lastPartialText,
          formatted: this.lastPartialFormatted,
        });
        ws.close(1000, "Complete");
        this.resetState();
        return;
      }

      if (hasTail) {
        const tailSecs = (tailAudio.byteLength / (this.audioConfig.sampleRate * 2)).toFixed(1);
        console.log(`[processAudio] tail STT on ${tailAudio.byteLength} bytes (~${tailSecs}s)`);
        sendMessage(ws, { type: "processing", stage: "stt" });

        const wavData = wrapPcmAsWav(tailAudio, this.audioConfig.sampleRate, this.audioConfig.channels, 16);
        const binaryStr = Array.from(wavData, (byte) => String.fromCharCode(byte)).join("");
        const audioBase64 = btoa(binaryStr);

        const sttStart = Date.now();
        try {
          const sttResult: { text: string } = await this.env.AI.run(
            this.env.STT_MODEL ?? "@cf/openai/whisper-large-v3-turbo",
            { audio: audioBase64 },
          );
          const tailText = sttResult.text.trim();
          console.log(`[processAudio] tail STT done in ${Date.now() - sttStart}ms: "${tailText.slice(0, 60)}"`);
          if (tailText) {
            this.committedText = this.committedText
              ? this.committedText + " " + tailText
              : tailText;
          }
        } catch (e) {
          console.log(`[processAudio] tail STT failed in ${Date.now() - sttStart}ms: ${e}`);
        }
      }

      sttText = this.committedText || this.lastPartialText;
    } else {
      console.log(`[processAudio] running full STT`);
      sendMessage(ws, { type: "processing", stage: "stt" });

      const combined = this.combineChunkRange(0, this.audioChunks.length);
      const wavData = wrapPcmAsWav(combined, this.audioConfig.sampleRate, this.audioConfig.channels, 16);
      const binaryStr = Array.from(wavData, (byte) => String.fromCharCode(byte)).join("");
      const audioBase64 = btoa(binaryStr);

      const sttStart = Date.now();
      try {
        const sttResult: { text: string } = await this.env.AI.run(
          this.env.STT_MODEL ?? "@cf/openai/whisper-large-v3-turbo",
          { audio: audioBase64 },
        );
        sttText = sttResult.text;
        console.log(`[processAudio] STT done in ${Date.now() - sttStart}ms`);
      } catch (e) {
        console.log(`[processAudio] STT failed in ${Date.now() - sttStart}ms: ${e}`);
        const msg = e instanceof Error ? e.message : "STT failed";
        sendMessage(ws, { type: "error", code: "stt_failed", message: msg });
        ws.close(1011, "STT failed");
        this.resetState();
        return;
      }
    }

    // Format — try checkpoint shortcut first (only format the tail after last sentence boundary)
    let formatted: string;
    sendMessage(ws, { type: "processing", stage: "format" });

    if (this.checkpointFormatted && this.checkpointRaw && sttText.startsWith(this.checkpointRaw)) {
      const tailRaw = sttText.slice(this.checkpointRaw.length).trim();
      if (!tailRaw) {
        console.log(`[processAudio] checkpoint covers everything, no tail to format`);
        formatted = this.checkpointFormatted;
      } else {
        console.log(`[processAudio] checkpoint hit, formatting tail only: "${tailRaw.slice(0, 60)}"`);
        const fmtStart = Date.now();
        try {
          const tailResult = await this.env.AI.run(
            this.env.FORMAT_MODEL ?? "@cf/meta/llama-3.2-3b-instruct",
            {
              messages: [
                { role: "system", content: buildSystemPrompt(this.focusContext) },
                { role: "user", content: buildUserMessage(tailRaw, this.focusContext) },
              ],
              temperature: 0,
            },
          );
          const tailFormatted = extractText(tailResult) ?? tailRaw;
          formatted = `${this.checkpointFormatted} ${tailFormatted}`;
          console.log(`[processAudio] tail format done in ${Date.now() - fmtStart}ms`);
        } catch {
          formatted = `${this.checkpointFormatted} ${tailRaw}`;
          console.log(`[processAudio] tail format failed, using raw tail`);
        }
      }
    } else {
      if (this.checkpointRaw) {
        console.log(`[processAudio] checkpoint miss (raw text diverged), full format`);
      }
      const fmtStart = Date.now();
      try {
        const formatResult = await this.env.AI.run(
          this.env.FORMAT_MODEL ?? "@cf/meta/llama-3.2-3b-instruct",
          {
            messages: [
              { role: "system", content: buildSystemPrompt(this.focusContext) },
              { role: "user", content: buildUserMessage(sttText, this.focusContext) },
            ],
            temperature: 0,
          },
        );
        formatted = extractText(formatResult) ?? sttText;
        console.log(`[processAudio] format done in ${Date.now() - fmtStart}ms`);
      } catch (e) {
        console.log(`[processAudio] format failed: ${e}`);
        const msg = e instanceof Error ? e.message : "Formatting failed";
        sendMessage(ws, { type: "error", code: "format_failed", message: msg });
        ws.close(1011, "Format failed");
        this.resetState();
        return;
      }
    }

    console.log(`[processAudio] total: ${Date.now() - t0}ms`);
    sendMessage(ws, {
      type: "result",
      raw: sttText,
      formatted,
    });

    ws.close(1000, "Complete");
    this.resetState();
  }
}
