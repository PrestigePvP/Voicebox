export type ErrorCode = "auth_failed" | "audio_too_large" | "stt_failed" | "format_failed" | "internal";

export type ClientMessage =
  | { type: "configure"; audio?: Partial<AudioConfig>; context?: Partial<FocusContext>; streamingStt?: boolean }
  | { type: "audio_end" }
  | { type: "cancel" };

export type ServerMessage =
  | { type: "ready" }
  | { type: "processing"; stage: "stt" | "format" }
  | { type: "partial"; text: string }
  | { type: "result"; raw: string; formatted: string }
  | { type: "error"; code: ErrorCode; message: string };

export interface AudioConfig {
  sampleRate: number;
  channels: number;
  encoding: string;
}

export interface FocusContext {
  appName: string;
  bundleID: string;
  elementRole: string;
  title: string;
  placeholder: string;
  value: string;
}