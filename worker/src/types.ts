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

// --- Meeting Mode HTTP API ---

export interface Turn {
  start: number;
  end: number;
  speaker: number;
  text: string;
}

export interface UploadInitRequest {
  sizeBytes: number;
}

export interface UploadInitResponse {
  key: string;
  uploadId: string;
}

export interface UploadPartResponse {
  partNumber: number;
  etag: string;
}

export interface UploadCompleteRequest {
  key: string;
  uploadId: string;
  parts: { partNumber: number; etag: string }[];
}

export interface UploadCompleteResponse {
  sizeBytes: number;
}

export interface TranscribeRequest {
  key: string;
}

export interface TranscribeResponse {
  durationSec: number;
  model: string;
  speakerCount: number;
  turns: Turn[];
}

export interface EnrichRequest {
  turns: Turn[];
}

export interface EnrichResponse {
  title: string | null;
  summary: string | null;
  speakers: Record<string, string | null>;
  model: string;
}

export interface ApiError {
  error: string;
  message: string;
}