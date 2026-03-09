# VoiceBox

Voice-to-text tool that captures speech, transcribes it via Whisper, and formats the output with an LLM. Press a hotkey, speak, release — formatted text lands in your clipboard and is auto-pasted into whatever you were typing in.

## How It Works

```
┌──────────────────────┐  PCM chunks   ┌──────────────────────────────────┐   formatted
│  Wails Desktop App   │ ──WebSocket──▶ │  Cloudflare Worker (Durable Obj) │ ──text────▶ Clipboard → Auto-paste
│  (Go + React WebView)│ ◀─────────────│  Whisper STT → LLM Formatter     │
└──────────────────────┘               └──────────────────────────────────┘
```

1. Hold **Ctrl+Cmd** — focus context is captured, recording starts, overlay appears at top-center
2. Speak into your microphone (voice level meter shows input)
3. Release **Ctrl+Cmd** — audio streams to the cloud
4. Whisper transcribes, LLM formats, result is copied to clipboard and auto-pasted into the originating app

## Project Structure

```
voicebox/
├── main.go                 # Wails entrypoint, app menu
├── app.go                  # App lifecycle, hotkey handlers, pipeline orchestration
├── window_darwin.go        # macOS window management (overlay, settings, dock click)
├── window_other.go         # Stub for non-macOS builds
├── internal/
│   ├── audio/              # PCM audio capture (malgo/miniaudio), RMS level
│   ├── pipeline/           # WebSocket client, streams audio + focus context to worker
│   ├── accessibility/      # macOS AX API: focused element context + auto-paste (Cmd+V)
│   ├── config/             # TOML config loading and saving
│   ├── hotkey/             # Global hotkey registration
│   ├── stt/                # STT provider interface (stubs)
│   └── formatter/          # LLM formatting provider interface (stubs)
├── frontend/               # React + Tailwind overlay UI (Vite)
│   └── src/
│       ├── App.tsx         # Routes between settings mode and overlay mode
│       ├── components/
│       │   ├── settings-form.tsx  # Config editor (react-hook-form + zod)
│       │   └── title-bar.tsx      # Frameless title bar with drag region
│       └── hooks/
│           ├── use-voicebox.ts    # voicebox:state / voicebox:mode / voicebox:level events
│           └── use-config.ts      # GetConfig / SaveConfig / GetConfigPath bindings
├── worker/                 # Cloudflare Worker (TypeScript)
│   ├── src/
│   │   ├── index.ts        # Router: /ws (WebSocket), /health
│   │   ├── session.ts      # Durable Object: audio accumulation + AI pipeline
│   │   ├── prompt.ts       # System prompt + user message builder
│   │   ├── wav.ts          # PCM-to-WAV wrapper
│   │   └── types.ts        # Shared types
│   ├── test/               # Vitest tests
│   └── wrangler.jsonc      # Worker configuration
├── go.mod
└── voicebox.toml           # User config (gitignored)
```

## Setup

### Prerequisites

- Go 1.24+
- Node.js + pnpm
- [Wails v2](https://wails.io/) CLI
- A Cloudflare account with Workers AI access
- macOS (accessibility permission required for auto-paste)

### Deploy the Worker

```bash
cd worker
pnpm install
wrangler secret put VOICEBOX_TOKEN     # set a shared secret
pnpm deploy
```

### Configure the Desktop Client

On first launch, VoiceBox opens a settings window. You can also create the config manually at `~/.config/voicebox/voicebox.toml`:

```toml
[provider]
mode = "cloud"

[cloud]
worker_url = "https://voicebox.<your-subdomain>.workers.dev"
token = "your-shared-secret"

[audio]
sample_rate = 16000
channels = 1
chunk_size = 4096

[hotkey]
record = "ctrl+cmd"
```

Config is loaded from (in order): `~/.config/voicebox/voicebox.toml`, next to the binary, then `./voicebox.toml`.

### macOS Accessibility Permission

Auto-paste requires macOS Accessibility access. On first use, macOS will prompt for permission, or you can grant it manually in **System Settings → Privacy & Security → Accessibility**.

### Build and Run

```bash
wails dev      # dev mode with hot reload
wails build    # production binary
```

## Window Modes

**Settings** (700×450, centered): Opens on launch, dock click, or via the Recording menu. Edit config here.

**Overlay** (160×48, top-center, floating): Appears during recording. Shows recording indicator with voice level meter, spinner while processing, checkmark on success.

## WebSocket Protocol

Client connects to `GET /ws?token=<auth-token>`.

After receiving `{"type":"ready"}`, the client sends a `configure` message with audio and focus context, then streams binary PCM chunks:

```
Client                          Server
  │── connect /ws?token=... ──────▶│
  │◀── {"type":"ready"} ──────────│
  │── {"type":"configure", ...} ──▶│
  │── [binary PCM chunk] ─────────▶│
  │── [binary PCM chunk] ─────────▶│
  │── {"type":"audio_end"} ───────▶│
  │◀── {"type":"processing",...} ──│
  │◀── {"type":"result",...} ──────│
```

The `configure` message carries audio params and focused element context (app name, bundle ID, element role, title, placeholder, current value) used by the LLM formatter to tailor output.

## Cloud Backend

- **STT**: `@cf/openai/whisper-large-v3-turbo`
- **Formatter**: `@cf/qwen/qwen3-30b-a3b-fp8`

## Local Backend (Phase 2)

- **STT**: faster-whisper
- **Formatter**: Ollama
- Provider interfaces exist at `internal/stt/` and `internal/formatter/`

## Audio Specs

- 16kHz sample rate, mono, PCM signed 16-bit LE
- ~4096 byte chunks (~128ms each)
- Max recording: ~25 MiB (~13 minutes)

## Development

```bash
# Desktop app
wails dev                              # dev server (Go + Vite hot reload)
wails build                            # production build
go vet ./...                           # lint Go
go test ./internal/...                 # test Go

# Frontend
cd frontend && pnpm install && pnpm build

# Worker
cd worker
pnpm dev                               # local dev server
pnpm lint                              # type-check
pnpm format                            # prettier
pnpm test                              # vitest
pnpm deploy                            # deploy to Cloudflare
```
