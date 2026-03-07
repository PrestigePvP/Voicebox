import { useVoiceBox } from "./hooks/use-voicebox";

const BAR_WEIGHTS = [0.5, 0.75, 1.0, 0.75, 0.5];

const VoiceMeter = ({ level }: { level: number }) => (
  <div className="flex items-center gap-[3px] h-5">
    {BAR_WEIGHTS.map((w, i) => (
      <div
        key={i}
        className="w-[3px] rounded-full bg-red-400 transition-[height] duration-75"
        style={{ height: `${Math.max(3, level * w * 20)}px` }}
      />
    ))}
  </div>
);

const App = () => {
  const { uiState, level } = useVoiceBox();

  if (uiState.state === "copied") {
    return (
      <div className="flex h-screen w-screen items-center justify-center">
        <div className="flex items-center justify-center rounded-full bg-zinc-900/90 px-4 py-2">
          <svg className="h-4 w-4 text-green-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2.5}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M5 13l4 4L19 7" />
          </svg>
        </div>
      </div>
    );
  }

  if (uiState.state === "error") {
    return (
      <div className="flex h-screen w-screen items-center justify-center">
        <div className="flex items-center justify-center rounded-full bg-zinc-900/90 px-4 py-2">
          <svg className="h-4 w-4 text-red-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2.5}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </div>
      </div>
    );
  }

  if (uiState.state === "recording") {
    return (
      <div className="flex h-screen w-screen items-center justify-center">
        <div className="flex items-center gap-2.5 rounded-full bg-zinc-900/90 px-4 py-2">
          <span className="relative flex h-2 w-2">
            <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-red-400 opacity-75" />
            <span className="relative inline-flex h-2 w-2 rounded-full bg-red-500" />
          </span>
          <VoiceMeter level={level} />
        </div>
      </div>
    );
  }

  if (uiState.state === "processing") {
    return (
      <div className="flex h-screen w-screen items-center justify-center">
        <div className="flex items-center justify-center rounded-full bg-zinc-900/90 px-4 py-2">
          <svg className="h-4 w-4 animate-spin text-zinc-400" fill="none" viewBox="0 0 24 24">
            <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
            <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
          </svg>
        </div>
      </div>
    );
  }

  return <div className="h-screen w-screen" />;
};

export default App;
