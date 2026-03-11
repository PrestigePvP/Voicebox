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
  const [partialText, setPartialText] = useState("");
  const [appIcon, setAppIcon] = useState<string | null>(null);
  const prevPartial = useRef("");
  const levelDecay = useRef<number>(0);

  useEffect(() => {
    const unlisteners = Promise.all([
      listen<UIState>("voicebox:state", (e) => {
        setUIState(e.payload);
        if (e.payload.state !== "recording") {
          setPartialText("");
          prevPartial.current = "";
        }
        if (e.payload.state === "idle") {
          setAppIcon(null);
        }
      }),
      listen<number>("voicebox:level", (e) => {
        const clamped = Math.min(1, Math.pow(e.payload * 8, 0.3));
        setLevel(clamped);
        cancelAnimationFrame(levelDecay.current);
        levelDecay.current = requestAnimationFrame(() => {
          setLevel((prev) => prev * 0.85);
        });
      }),
      listen<string>("voicebox:partial", (e) => {
        const full = e.payload;
        const prev = prevPartial.current;
        // Show only the new portion of the transcription
        const newText = full.startsWith(prev)
          ? full.slice(prev.length).trim()
          : full;
        setPartialText(newText || full);
        prevPartial.current = full;
      }),
      listen<string>("voicebox:icon", (e) => {
        setAppIcon(`data:image/png;base64,${e.payload}`);
      }),
    ]);

    return () => {
      unlisteners.then((fns) => fns.forEach((fn) => fn()));
      cancelAnimationFrame(levelDecay.current);
    };
  }, []);

  return { uiState, level, partialText, appIcon };
};
