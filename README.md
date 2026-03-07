# VoiceBox

Voice-to-text tool that captures speech, transcribes it via Whisper, and formats the output with an LLM. Press a hotkey, speak, press again — formatted text lands in your clipboard.

## How It Works

```
┌──────────┐  PCM chunks   ┌──────────────────────────────────┐   formatted
│  Desktop │ ──WebSocket──▶ │  Cloudflare Worker (Durable Obj) │ ──text────▶ Clipboard
│  Binary  │ ◀─────────────│  Whisper STT → LLM Formatter     │
└──────────┘               └──────────────────────────────────┘
```

1. Press **Ctrl+Shift+R** — recording starts
2. Speak into your microphone
3. Press **Ctrl+Shift+R** again — recording stops, audio streams to the cloud
4. Whisper transcribes, LLM formats, result is copied to clipboard

## Project Structure

```
voicebox/
├── cmd/voicebox/        # Go binary entrypoint
├── internal/
│   ├── audio/           # PCM audio capture (malgo/miniaudio)
│   ├── pipeline/        # WebSocket client, streams audio to worker
│   ├── config/          # TOML config loading
│   ├── ui/              # System tray, hotkeys, state management
│   ├── stt/             # Speech-to-text provider interface (stubs)
│   └── formatter/       # LLM formatting provider interface (stubs)
├── worker/              # Cloudflare Worker (TypeScript)
│   └── src/
│       ├── index.ts     # Router: /ws (WebSocket), /health
│       ├── session.ts   # Durable Object: audio accumulation + AI pipeline
│       ├── wav.ts       # PCM-to-WAV wrapper
│       └── types.ts     # Shared types
├── go.mod
└── voicebox.toml        # User config (gitignored)
```

## Setup

### Prerequisites

- Go 1.24+
- Node.js + pnpm
- A Cloudflare account with Workers AI access

### Deploy the Worker

```bash
cd worker
pnpm install
wrangler secret put VOICEBOX_TOKEN    # set a shared secret
pnpm deploy
```

### Configure the Desktop Client

Create `voicebox.toml` in the project root or `~/.config/voicebox/voicebox.toml`:

```toml
[provider]
mode = "cloud"

[cloud]
worker_url = "wss://voicebox.<your-subdomain>.workers.dev"
token = "your-shared-secret"
stt_model = "@cf/openai/whisper-large-v3-turbo"
formatter_model = "@cf/ibm-granite/granite-4.0-h-micro"

[audio]
sample_rate = 16000
channels = 1
chunk_size = 4096

[hotkey]
record = "ctrl+shift+r"
```

### Build and Run

```bash
go build ./cmd/voicebox
./voicebox
```

A system tray icon appears. Use the hotkey or tray menu to record.

## Cloud Backend

- **STT**: `@cf/openai/whisper-large-v3-turbo`
- **Formatter**: `@cf/ibm-granite/granite-4.0-h-micro`
- Falls within free tier for typical dictation usage (~13 min/day)

## Local Backend (Phase 2)

- **STT**: faster-whisper (medium model, ~2GB VRAM)
- **Formatter**: Ollama (Qwen3 0.6B)
- Provider interfaces exist, implementations coming in Phase 2

## Audio Specs

- 16kHz sample rate, mono, PCM signed 16-bit LE
- ~4096 byte chunks (~128ms each)
- Max recording: ~25 MiB (~13 minutes)

## Development

```bash
# Go
go build ./cmd/voicebox    # build binary
go test ./...              # run tests
go vet ./...               # lint

# Worker
cd worker
pnpm dev                   # local dev server
pnpm lint                  # type-check
pnpm deploy                # deploy to Cloudflare
```
