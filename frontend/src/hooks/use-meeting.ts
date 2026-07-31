import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { MeetingUIState } from "../lib/meeting-types";

export const useMeeting = () => {
  const [meetingState, setMeetingState] = useState<MeetingUIState>({ state: "idle" });
  const [level, setLevel] = useState(0);
  const [nowMs, setNowMs] = useState(0);
  const [actionError, setActionError] = useState<string | null>(null);

  useEffect(() => {
    invoke<MeetingUIState>("get_meeting_state")
      .then(setMeetingState)
      .catch(() => {});

    const unlistenState = listen<MeetingUIState>("voicebox:meeting-state", (event) => {
      setMeetingState(event.payload);
    });
    const unlistenLevel = listen<number>("voicebox:level", (event) => {
      setLevel(event.payload);
    });

    return () => {
      unlistenState.then((fn) => fn());
      unlistenLevel.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    if (meetingState.state !== "recording") return;
    const timer = setInterval(() => setNowMs(Date.now()), 500);
    return () => clearInterval(timer);
  }, [meetingState.state]);

  const elapsedSec =
    meetingState.state === "recording"
      ? Math.max(0, (nowMs - meetingState.startedAtMs) / 1000)
      : 0;

  const wrap = useCallback(async (command: string, args?: Record<string, unknown>) => {
    setActionError(null);
    try {
      await invoke(command, args);
    } catch (err) {
      setActionError(String(err));
    }
  }, []);

  const start = useCallback(() => wrap("start_meeting"), [wrap]);
  const stop = useCallback(() => wrap("stop_meeting"), [wrap]);
  const retry = useCallback((id: string) => wrap("retry_meeting", { id }), [wrap]);

  return { meetingState, level, elapsedSec, actionError, start, stop, retry };
};
