import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useVoiceBox } from "./hooks/use-voicebox";
import { useConfig } from "./hooks/use-config";
import { cn } from "./lib/utils";
import TitleBar from "./components/title-bar";
import SettingsForm from "./components/settings-form";

const BAR_WEIGHTS = [0.3, 0.5, 0.7, 1.0, 0.7, 0.5, 0.3];

const VoiceMeter = ({ level }: { level: number }) => (
  <div className="flex items-center gap-[3px] h-8">
    {BAR_WEIGHTS.map((w, i) => (
      <div
        key={i}
        className="w-1 rounded-full bg-red-400 transition-[height] duration-75"
        style={{ height: `${Math.max(4, level * w * 32)}px` }}
      />
    ))}
  </div>
);

const isBottom = (pos: string) => pos.startsWith("bottom");

const Pill = ({ children }: { children: React.ReactNode }) => (
  <div className="flex items-center justify-center rounded-full bg-zinc-900/90 px-4 py-2">
    {children}
  </div>
);

const AppIcon = ({ src }: { src: string }) => (
  <img src={src} alt="" className="h-5 w-5 rounded" />
);

const Overlay = ({ uiState, level, partialText, position, appIcon }: {
  uiState: ReturnType<typeof useVoiceBox>["uiState"];
  level: number;
  partialText: string;
  position: string;
  appIcon: string | null;
}) => {
  const bottom = isBottom(position);
  const anchor = bottom ? "justify-end" : "justify-start";

  if (uiState.state === "copied") {
    return (
      <div className={cn("flex h-screen w-screen flex-col items-center", anchor)}>
        <Pill>
          {appIcon && <AppIcon src={appIcon} />}
          <svg className="h-4 w-4 text-green-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2.5}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M5 13l4 4L19 7" />
          </svg>
        </Pill>
      </div>
    );
  }

  if (uiState.state === "error") {
    return (
      <div className={cn("flex h-screen w-screen flex-col items-center", anchor)}>
        <Pill>
          {appIcon && <AppIcon src={appIcon} />}
          <svg className="h-4 w-4 text-red-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2.5}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </Pill>
      </div>
    );
  }

  if (uiState.state === "recording") {
    return (
      <div className={cn("flex h-screen w-screen flex-col items-center gap-2", anchor)}>
        {bottom && partialText && (
          <div className="max-w-[280px] rounded-lg bg-zinc-900/90 px-3 py-1.5">
            <p className="text-xs text-zinc-300 truncate">{partialText}</p>
          </div>
        )}
        <Pill>
          {appIcon && <AppIcon src={appIcon} />}
          <span className="relative flex h-2 w-2 mr-2.5">
            <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-red-400 opacity-75" />
            <span className="relative inline-flex h-2 w-2 rounded-full bg-red-500" />
          </span>
          <VoiceMeter level={level} />
        </Pill>
        {!bottom && partialText && (
          <div className="max-w-[280px] rounded-lg bg-zinc-900/90 px-3 py-1.5">
            <p className="text-xs text-zinc-300 truncate">{partialText}</p>
          </div>
        )}
      </div>
    );
  }

  if (uiState.state === "processing") {
    return (
      <div className={cn("flex h-screen w-screen flex-col items-center", anchor)}>
        <Pill>
          {appIcon && <AppIcon src={appIcon} />}
          <svg className="h-4 w-4 animate-spin text-zinc-400" fill="none" viewBox="0 0 24 24">
            <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
            <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
          </svg>
        </Pill>
      </div>
    );
  }

  return <div className="h-screen w-screen" />;
};

const windowLabel = getCurrentWebviewWindow().label;

const App = () => {
  const { uiState, level, partialText, appIcon } = useVoiceBox();
  const { config } = useConfig();

  if (windowLabel === "main") {
    return (
      <div className="flex flex-col h-screen bg-zinc-900 text-zinc-100 rounded-lg overflow-hidden">
        <TitleBar />
        <div className="flex-1 overflow-y-auto">
          <SettingsForm />
        </div>
      </div>
    );
  }

  return <Overlay uiState={uiState} level={level} partialText={partialText} position={config?.overlay_position ?? "top_center"} appIcon={appIcon} />;
};

export default App;
