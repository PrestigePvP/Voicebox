import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import TitleBar from "../title-bar";
import RecordPanel from "./record-panel";
import MeetingList from "./meeting-list";
import MeetingDetail from "./meeting-detail";
import { useMeeting } from "../../hooks/use-meeting";
import type { MeetingDoc, MeetingSummary } from "../../lib/meeting-types";

const MeetingsApp = () => {
  const { meetingState, level, elapsedSec, actionError, start, stop, retry } = useMeeting();
  const [meetings, setMeetings] = useState<MeetingSummary[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [docState, setDocState] = useState<{ id: string; doc: MeetingDoc } | null>(null);
  const doc = docState && docState.id === selectedId ? docState.doc : null;
  const setDoc = useCallback(
    (updated: MeetingDoc) => setDocState({ id: updated.id, doc: updated }),
    [],
  );

  const refreshList = useCallback(async () => {
    try {
      const list = await invoke<MeetingSummary[]>("list_meetings");
      setMeetings(list);
      setSelectedId((current) => current ?? list[0]?.id ?? null);
    } catch (err) {
      console.error("list_meetings failed:", err);
    }
  }, []);

  useEffect(() => {
    refreshList();
  }, [refreshList]);

  // New recordings and finished/failed processing change the list and the
  // selected doc on disk — refetch on those transitions.
  useEffect(() => {
    if (
      meetingState.state === "recording" ||
      meetingState.state === "complete" ||
      meetingState.state === "error"
    ) {
      refreshList();
      if (
        meetingState.state !== "recording" &&
        "meetingId" in meetingState &&
        meetingState.meetingId === selectedId
      ) {
        invoke<MeetingDoc>("load_meeting", { id: meetingState.meetingId })
          .then(setDoc)
          .catch(() => {});
      }
    }
  }, [meetingState, refreshList, selectedId, setDoc]);

  useEffect(() => {
    if (!selectedId) return;
    let cancelled = false;
    invoke<MeetingDoc>("load_meeting", { id: selectedId })
      .then((loaded) => {
        if (!cancelled) setDocState({ id: selectedId, doc: loaded });
      })
      .catch((err) => console.error("load_meeting failed:", err));
    return () => {
      cancelled = true;
    };
  }, [selectedId]);

  return (
    <div className="flex h-screen flex-col overflow-hidden rounded-lg bg-zinc-900 text-zinc-100">
      <TitleBar title="VoiceBox Meetings" />
      <RecordPanel
        meetingState={meetingState}
        level={level}
        elapsedSec={elapsedSec}
        actionError={actionError}
        onStart={start}
        onStop={stop}
      />
      <div className="flex min-h-0 flex-1">
        <div className="w-64 shrink-0 border-r border-zinc-800 overflow-y-auto">
          <MeetingList meetings={meetings} selectedId={selectedId} onSelect={setSelectedId} />
        </div>
        <div className="min-w-0 flex-1">
          {doc ? (
            <MeetingDetail doc={doc} onDocUpdated={setDoc} onRetry={retry} />
          ) : (
            <div className="flex h-full items-center justify-center">
              <p className="text-sm text-zinc-500">Select a meeting to view its transcript.</p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

export default MeetingsApp;
