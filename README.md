# VoiceBox

Voice-to-text tool that captures speech, transcribes it via Whisper, and formats the output with an LLM. Press a hotkey, speak, release — formatted text lands in your clipboard.

## How It Works

```
┌──────────────────────┐  PCM chunks   ┌──────────────────────────────────┐   formatted
│  Wails Desktop App   │ ──WebSocket──▶ │  Cloudflare Worker (Durable Obj) │ ──text────▶ Clipboard
│  (Go + React WebView)│ ◀─────────────│  Whisper STT → LLM Formatter     │
└──────────────────────┘               └──────────────────────────────────┘
```

1. Hold **Ctrl+Shift+R** — recording starts, overlay appears
2. Speak into your microphone
3. Release **Ctrl+Shift+R** — audio streams to the cloud
4. Whisper transcribes, LLM formats, result is copied to clipboard, overlay auto-hides

## Project Structure

```
voicebox/
├── main.go                 # Wails entrypoint, app menu
├── app.go                  # App lifecycle, hotkey handlers, pipeline orchestration
├── internal/
│   ├── audio/              # PCM audio capture (malgo/miniaudio)
│   ├── pipeline/           # WebSocket client, streams audio to worker
│   ├── config/             # TOML config loading
│   ├── hotkey/             # Global hotkey registration
│   ├── stt/                # STT provider interface (stubs)
│   └── formatter/          # LLM formatting provider interface (stubs)
├── frontend/               # React + Tailwind overlay UI (Vite)
│   └── src/
│       ├── App.tsx         # Frameless overlay (recording/processing/copied/error)
│       └── hooks/          # Event listeners for Go backend state
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

### Deploy the Worker

```bash
cd worker
pnpm install
pnpm types                             # generate runtime types
wrangler secret put VOICEBOX_TOKEN     # set a shared secret
pnpm deploy
```

### Configure the Desktop Client

Create `voicebox.toml` in the project root or `~/.config/voicebox/voicebox.toml`:

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
record = "ctrl+shift+r"
```

### Build and Run

```bash
wails dev      # dev mode with hot reload
wails build    # production binary
```

## WebSocket Protocol

Client connects to `GET /ws?token=<auth-token>`.

After receiving `{"type":"ready"}`, the client sends a `configure` message with audio and context settings, then streams binary PCM chunks:

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

The `configure` message is optional (defaults apply) and can be sent at any point during the session.

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
pnpm types                             # generate wrangler types
pnpm lint                              # type-check
pnpm format                            # prettier
pnpm test                              # vitest
pnpm deploy                            # deploy to Cloudflare
```
