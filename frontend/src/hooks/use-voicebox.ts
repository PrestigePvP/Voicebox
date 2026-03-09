import { useEffect, useRef, useState } from "react";

export type UIState =
  | { state: "idle" }
  | { state: "recording"; startTime: number }
  | { state: "processing"; stage: string }
  | { state: "copied" }
  | { state: "error"; message: string };

export type AppMode = "settings" | "overlay";

declare global {
  interface Window {
    runtime: {
      EventsOn: (
        event: string,
        callback: (...args: unknown[]) => void,
      ) => () => void;
      WindowHide: () => void;
    };
  }
}

export const useVoiceBox = () => {
  const [uiState, setUIState] = useState<UIState>({ state: "idle" });
  const [mode, setMode] = useState<AppMode>("settings");
  const [level, setLevel] = useState(0);
  const levelDecay = useRef<number>(0);

  useEffect(() => {
    const cancelState = window.runtime.EventsOn(
      "voicebox:state",
      (...args: unknown[]) => {
        setUIState(args[0] as UIState);
      },
    );

    const cancelMode = window.runtime.EventsOn(
      "voicebox:mode",
      (...args: unknown[]) => {
        setMode(args[0] as AppMode);
      },
    );

    const cancelLevel = window.runtime.EventsOn(
      "voicebox:level",
      (...args: unknown[]) => {
        const raw = args[0] as number;
        const clamped = Math.min(1, raw * 3);
        setLevel(clamped);
        cancelAnimationFrame(levelDecay.current);
        levelDecay.current = requestAnimationFrame(() => {
          setLevel((prev) => prev * 0.85);
        });
      },
    );

    return () => {
      cancelState();
      cancelMode();
      cancelLevel();
      cancelAnimationFrame(levelDecay.current);
    };
  }, []);

  return { uiState, mode, level };
};
