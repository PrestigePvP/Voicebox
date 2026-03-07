import { useEffect, useRef, useState } from "react";

export type UIState =
  | { state: "idle" }
  | { state: "recording"; startTime: number }
  | { state: "processing"; stage: string }
  | { state: "copied" }
  | { state: "error"; message: string };

declare global {
  interface Window {
    runtime: {
      EventsOn: (
        event: string,
        callback: (...args: unknown[]) => void,
      ) => () => void;
    };
  }
}

export const useVoiceBox = () => {
  const [uiState, setUIState] = useState<UIState>({ state: "idle" });
  const [level, setLevel] = useState(0);
  const levelDecay = useRef<number>(0);

  useEffect(() => {
    const cancelState = window.runtime.EventsOn(
      "voicebox:state",
      (...args: unknown[]) => {
        setUIState(args[0] as UIState);
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
      cancelLevel();
      cancelAnimationFrame(levelDecay.current);
    };
  }, []);

  return { uiState, level };
};
