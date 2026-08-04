import { cn } from "../../lib/utils";
import { formatClock, type MeetingUIState } from "../../lib/meeting-types";
import VoiceMeter from "../voice-meter";

const Spinner = () => (
  <svg className="h-4 w-4 animate-spin text-zinc-400" fill="none" viewBox="0 0 24 24">
    <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
    <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
  </svg>
);

const RecordDot = () => (
  <span className="relative flex h-2.5 w-2.5">
    <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-red-400 opacity-75" />
    <span className="relative inline-flex h-2.5 w-2.5 rounded-full bg-red-500" />
  </span>
);

const stageLabel = (state: MeetingUIState): string | null => {
  switch (state.state) {
    case "uploading":
      return `Uploading… ${Math.round(state.progress * 100)}%`;
    case "transcribing":
      return "Transcribing…";
    case "formatting":
      return "Formatting…";
    default:
      return null;
  }
};

const RecordPanel = ({
  meetingState,
  level,
  elapsedSec,
  actionError,
  onStart,
  onStop,
}: {
  meetingState: MeetingUIState;
  level: number;
  elapsedSec: number;
  actionError: string | null;
  onStart: () => void;
  onStop: () => void;
}) => {
  const recording = meetingState.state === "recording";
  const busy = stageLabel(meetingState);

  return (
    <div className="border-b border-zinc-800 px-4 py-3">
      <div className="flex items-center gap-4">
        <button
          onClick={recording ? onStop : onStart}
          disabled={busy !== null}
          className={cn(
            "rounded-md px-4 py-2 text-sm font-medium transition-colors",
            recording
              ? "bg-red-600 hover:bg-red-500 text-white"
              : "bg-blue-600 hover:bg-blue-500 text-white",
            busy !== null && "opacity-50 cursor-not-allowed",
          )}
        >
          {recording ? "Stop Recording" : "Record Meeting"}
        </button>

        {recording && (
          <div className="flex items-center gap-3">
            <RecordDot />
            <span className="font-mono text-sm text-zinc-200 tabular-nums">
              {formatClock(elapsedSec)}
            </span>
            <VoiceMeter level={level} />
          </div>
        )}

        {busy && (
          <div className="flex items-center gap-2 text-sm text-zinc-400">
            <Spinner />
            <span>{busy}</span>
            {meetingState.state === "uploading" && (
              <div className="h-1.5 w-32 overflow-hidden rounded-full bg-zinc-800">
                <div
                  className="h-full rounded-full bg-blue-500 transition-[width] duration-300"
                  style={{ width: `${Math.round(meetingState.progress * 100)}%` }}
                />
              </div>
            )}
          </div>
        )}

        {meetingState.state === "error" && (
          <p className="text-sm text-red-400 truncate">{meetingState.message}</p>
        )}
        {actionError && <p className="text-sm text-red-400 truncate">{actionError}</p>}
      </div>
    </div>
  );
};

export default RecordPanel;
