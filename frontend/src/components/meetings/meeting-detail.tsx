import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { cn } from "../../lib/utils";
import {
  displayName,
  formatClock,
  formatDate,
  formatDuration,
  speakerColor,
  type MeetingDoc,
} from "../../lib/meeting-types";

const SpeakerLegend = ({
  doc,
  onRenamed,
}: {
  doc: MeetingDoc;
  onRenamed: (doc: MeetingDoc) => void;
}) => {
  const [editing, setEditing] = useState<string | null>(null);
  const [draft, setDraft] = useState("");

  const commit = async (speakerId: string) => {
    setEditing(null);
    const name = draft.trim();
    if (!name || name === (doc.speakers[speakerId]?.name ?? "")) return;
    try {
      const updated = await invoke<MeetingDoc>("rename_speaker", {
        id: doc.id,
        speaker: speakerId,
        name,
      });
      onRenamed(updated);
    } catch (err) {
      console.error("rename_speaker failed:", err);
    }
  };

  return (
    <div className="flex flex-wrap gap-2">
      {Object.keys(doc.speakers)
        .sort((a, b) => Number(a) - Number(b))
        .map((id) => {
          const info = doc.speakers[id];
          const num = Number(id);
          return editing === id ? (
            <input
              key={id}
              autoFocus
              defaultValue={info.name ?? ""}
              placeholder={`Speaker ${num + 1}`}
              onChange={(e) => setDraft(e.target.value)}
              onBlur={() => commit(id)}
              onKeyDown={(e) => {
                if (e.key === "Enter") commit(id);
                if (e.key === "Escape") setEditing(null);
              }}
              className="w-32 rounded-full bg-zinc-800 px-3 py-1 text-xs text-zinc-100 outline-none ring-1 ring-blue-500"
            />
          ) : (
            <button
              key={id}
              title="Click to rename"
              onClick={() => {
                setDraft(info.name ?? "");
                setEditing(id);
              }}
              className={cn(
                "rounded-full px-3 py-1 text-xs font-medium transition-opacity hover:opacity-75",
                speakerColor(num),
              )}
            >
              {displayName(doc.speakers, num)}
              {info.name && info.inferred && <span className="ml-1 opacity-60">(auto)</span>}
            </button>
          );
        })}
    </div>
  );
};

const MeetingDetail = ({
  doc,
  onDocUpdated,
  onRetry,
}: {
  doc: MeetingDoc;
  onDocUpdated: (doc: MeetingDoc) => void;
  onRetry: (id: string) => void;
}) => (
  <div className="flex h-full flex-col overflow-y-auto px-6 py-4">
    <div className="mb-1 flex items-start justify-between gap-4">
      <h2 className="text-lg font-semibold text-zinc-100">
        {doc.title ?? `Meeting ${formatDate(doc.createdAtMs)}`}
      </h2>
      <div className="flex shrink-0 gap-2">
        {doc.status !== "complete" && (
          <button
            onClick={() => onRetry(doc.id)}
            className="rounded-md bg-blue-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-blue-500"
          >
            {doc.status === "error" ? "Retry Processing" : "Process"}
          </button>
        )}
        <button
          onClick={() => invoke("open_meetings_folder").catch(() => {})}
          className="rounded-md bg-zinc-800 px-3 py-1.5 text-xs text-zinc-300 hover:bg-zinc-700"
        >
          Reveal in Finder
        </button>
      </div>
    </div>

    <p className="mb-3 text-xs text-zinc-500">
      {formatDate(doc.createdAtMs)} · {formatDuration(doc.durationSec)} ·{" "}
      {Object.keys(doc.speakers).length || "?"} speakers
    </p>

    {doc.status === "error" && doc.error && (
      <div className="mb-4 rounded-md border border-red-500/30 bg-red-500/10 px-3 py-2 text-sm text-red-300">
        {doc.error}
      </div>
    )}

    {doc.summary && (
      <div className="mb-4 rounded-md bg-zinc-800/60 px-4 py-3">
        <p className="text-sm leading-relaxed text-zinc-300">{doc.summary}</p>
      </div>
    )}

    {doc.turns.length > 0 && (
      <>
        <div className="mb-4">
          <SpeakerLegend doc={doc} onRenamed={onDocUpdated} />
        </div>

        <div className="space-y-3 pb-6">
          {doc.turns.map((turn, i) => (
            <div key={i} className="flex gap-3">
              <div className="w-24 shrink-0 pt-0.5 text-right">
                <span className="font-mono text-[11px] text-zinc-600 tabular-nums">
                  {formatClock(turn.start)}
                </span>
              </div>
              <div>
                <span
                  className={cn(
                    "mr-2 inline-block rounded px-1.5 py-0.5 text-[11px] font-medium",
                    speakerColor(turn.speaker),
                  )}
                >
                  {displayName(doc.speakers, turn.speaker)}
                </span>
                <span className="text-sm leading-relaxed text-zinc-300">{turn.text}</span>
              </div>
            </div>
          ))}
        </div>
      </>
    )}

    {doc.status === "recorded" && doc.turns.length === 0 && (
      <p className="text-sm text-zinc-500">
        Audio recorded but not yet transcribed. Hit Process to upload and transcribe it.
      </p>
    )}
  </div>
);

export default MeetingDetail;
