import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

export type UIState =
  | { state: "idle" }
  | { state: "recording"; startTime: number }
  | { state: "processing"; stage: string }
  | { state: "copied" }
  | { state: "error"; message: string };

export const useVoiceBox = () => {
  const [uiState, setUIState] = useState<UIState>({ state: "idle" });
  const [level, setLevel] = useState(0);
  const levelDecay = useRef<number>(0);

  useEffect(() => {
    const unlisteners = Promise.all([
      listen<UIState>("voicebox:state", (e) => {
        setUIState(e.payload);
      }),
      listen<number>("voicebox:level", (e) => {
        const clamped = Math.min(1, e.payload * 3);
        setLevel(clamped);
        cancelAnimationFrame(levelDecay.current);
        levelDecay.current = requestAnimationFrame(() => {
          setLevel((prev) => prev * 0.85);
        });
      }),
    ]);

    return () => {
      unlisteners.then((fns) => fns.forEach((fn) => fn()));
      cancelAnimationFrame(levelDecay.current);
    };
  }, []);

  return { uiState, level };
};
