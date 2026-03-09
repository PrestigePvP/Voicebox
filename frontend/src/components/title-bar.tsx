const TitleBar = () => (
  <div
    className="flex items-center justify-between px-4 py-2 bg-zinc-800 select-none"
    style={{ "--wails-draggable": "drag" } as React.CSSProperties}
  >
    <span className="text-sm font-medium text-zinc-300">VoiceBox</span>
    <button
      onClick={() => window.runtime.WindowHide()}
      className="flex items-center justify-center w-6 h-6 rounded hover:bg-zinc-700 text-zinc-400 hover:text-zinc-200 transition-colors"
      style={{ "--wails-draggable": "no-drag" } as React.CSSProperties}
    >
      <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
        <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
      </svg>
    </button>
  </div>
);

export default TitleBar;
