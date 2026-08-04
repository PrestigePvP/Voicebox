export interface Turn {
  start: number;
  end: number;
  speaker: number;
  text: string;
}

export interface SpeakerInfo {
  name: string | null;
  inferred: boolean;
}

export type MeetingStatus = "recorded" | "complete" | "error";

export interface MeetingDoc {
  version: number;
  id: string;
  createdAtMs: number;
  durationSec: number;
  audioFile: string;
  status: MeetingStatus;
  error: string | null;
  title: string | null;
  summary: string | null;
  sttModel: string | null;
  enrichModel: string | null;
  speakers: Record<string, SpeakerInfo>;
  turns: Turn[];
}

export interface MeetingSummary {
  id: string;
  title: string | null;
  createdAtMs: number;
  durationSec: number;
  status: MeetingStatus;
}

export type MeetingUIState =
  | { state: "idle" }
  | { state: "recording"; meetingId: string; startedAtMs: number }
  | { state: "uploading"; meetingId: string; progress: number }
  | { state: "transcribing"; meetingId: string }
  | { state: "formatting"; meetingId: string }
  | { state: "complete"; meetingId: string }
  | { state: "error"; meetingId: string; message: string };

export const isMeetingBusy = (s: MeetingUIState): boolean =>
  s.state === "uploading" || s.state === "transcribing" || s.state === "formatting";

export const displayName = (
  speakers: Record<string, SpeakerInfo>,
  speakerId: number,
): string => speakers[String(speakerId)]?.name ?? `Speaker ${speakerId + 1}`;

export const SPEAKER_COLORS = [
  "bg-blue-500/20 text-blue-300",
  "bg-emerald-500/20 text-emerald-300",
  "bg-amber-500/20 text-amber-300",
  "bg-purple-500/20 text-purple-300",
  "bg-rose-500/20 text-rose-300",
  "bg-cyan-500/20 text-cyan-300",
];

export const speakerColor = (speaker: number): string =>
  SPEAKER_COLORS[speaker % SPEAKER_COLORS.length];

export const formatClock = (sec: number): string => {
  const s = Math.max(0, Math.floor(sec));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = s % 60;
  const mm = String(m).padStart(2, "0");
  const ss = String(r).padStart(2, "0");
  return h > 0 ? `${h}:${mm}:${ss}` : `${m}:${ss}`;
};

export const formatDuration = (sec: number): string => {
  const s = Math.max(0, Math.round(sec));
  if (s < 60) return `${s}s`;
  const h = Math.floor(s / 3600);
  const m = Math.round((s % 3600) / 60);
  return h > 0 ? `${h}h ${String(m).padStart(2, "0")}m` : `${m}m`;
};

export const formatDate = (ms: number): string =>
  new Date(ms).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
