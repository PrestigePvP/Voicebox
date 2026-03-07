export type ErrorCode = "auth_failed" | "audio_too_large" | "stt_failed" | "format_failed" | "internal";

export type ClientMessage =
  | { type: "audio_end" }
  | { type: "cancel" };

export type ServerMessage =
  | { type: "ready" }
  | { type: "processing"; stage: "stt" | "format" }
  | { type: "result"; raw: string; formatted: string }
  | { type: "error"; code: ErrorCode; message: string };

export interface AudioConfig {
  sampleRate: number;
  channels: number;
  encoding: string;
}

export interface Env {
  AI: Ai;
  TRANSCRIPTION_SESSION: DurableObjectNamespace;
  VOICEBOX_TOKEN: string;
}
