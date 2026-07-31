import { cn } from "../../lib/utils";
import {
  formatDate,
  formatDuration,
  type MeetingSummary,
} from "../../lib/meeting-types";

const statusBadge = (status: MeetingSummary["status"]) => {
  switch (status) {
    case "complete":
      return null;
    case "recorded":
      return <span className="rounded bg-amber-500/20 px-1.5 py-0.5 text-[10px] text-amber-300">unprocessed</span>;
    case "error":
      return <span className="rounded bg-red-500/20 px-1.5 py-0.5 text-[10px] text-red-300">error</span>;
  }
};

const MeetingList = ({
  meetings,
  selectedId,
  onSelect,
}: {
  meetings: MeetingSummary[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}) => {
  if (meetings.length === 0) {
    return (
      <div className="flex h-full items-center justify-center px-4 text-center">
        <p className="text-sm text-zinc-500">
          No meetings yet. Hit Record Meeting and set the laptop where it can hear everyone.
        </p>
      </div>
    );
  }

  return (
    <ul className="overflow-y-auto">
      {meetings.map((m) => (
        <li key={m.id}>
          <button
            onClick={() => onSelect(m.id)}
            className={cn(
              "w-full border-b border-zinc-800/60 px-4 py-3 text-left transition-colors hover:bg-zinc-800/50",
              selectedId === m.id && "bg-zinc-800",
            )}
          >
            <div className="flex items-center justify-between gap-2">
              <span className="truncate text-sm font-medium text-zinc-200">
                {m.title ?? `Meeting ${formatDate(m.createdAtMs)}`}
              </span>
              {statusBadge(m.status)}
            </div>
            <div className="mt-0.5 flex gap-2 text-xs text-zinc-500">
              <span>{formatDate(m.createdAtMs)}</span>
              <span>·</span>
              <span>{formatDuration(m.durationSec)}</span>
            </div>
          </button>
        </li>
      ))}
    </ul>
  );
};

export default MeetingList;
