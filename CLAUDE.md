## Overview

VoiceBox is a voice-to-text pipeline with a Wails v2 desktop app (Go + React frontend) and a Cloudflare Worker cloud backend. Press a hotkey to record speech. Audio is captured natively in Go via miniaudio, streamed via Go WebSocket client to a Cloudflare Worker that runs Whisper STT then LLM formatting, and copies the result to clipboard.

Local backend support (faster-whisper + Ollama) is planned for Phase 2. Provider stubs exist at `internal/stt/` and `internal/formatter/`.

## Commands

### Wails (root directory)
- `wails dev` - run dev server (Go + Vite hot reload)
- `wails build` - build standalone binary
- `go vet github.com/PrestigePvP/voicebox github.com/PrestigePvP/voicebox/internal/...` - lint Go code
- `go test github.com/PrestigePvP/voicebox/internal/...` - run Go tests
- Frontend must be built (`pnpm build` in `frontend/`) before `go build` due to `//go:embed all:frontend/dist`

### Frontend (`frontend/` directory)
- `pnpm install` - install dependencies
- `pnpm build` - production build
- `pnpm dev` - Vite dev server (used by `wails dev`)

### Cloudflare Worker (`worker/` directory)
- `pnpm install` - install dependencies
- `pnpm dev` - local dev server (wrangler)
- `pnpm deploy` - deploy to Cloudflare
- `pnpm lint` - type-check with tsc

## Architecture

### Go Backend
- **`main.go`** - Wails entrypoint. Configures window (320x120, frameless, hidden by default), app menu, and binds App struct.
- **`app.go`** - App struct with `startup`/`shutdown` lifecycle, hotkey-driven recording orchestration, audio capture, WebSocket pipeline, clipboard copy, and window show/hide.
- **`internal/audio/`** - Audio capture using `malgo` (miniaudio Go bindings). Records PCM s16le at configured sample rate. Emits fixed-size chunks via channel.
- **`internal/pipeline/`** - Go WebSocket client. Connects to Worker, streams PCM chunks, sends `audio_end`, receives transcription result.
- **`internal/hotkey/`** - `ParseHotkey` and `RegisterHotkey` using `golang.design/x/hotkey` for system-wide hotkey registration.
- **`internal/config/`** - TOML config loading with defaults. Reads `voicebox.toml`.
- **`internal/stt/`** - STT provider interface + stubs (CloudProvider, LocalProvider).
- **`internal/formatter/`** - Formatter provider interface + stubs (CloudProvider, LocalProvider).

### Frontend (React + TypeScript + Tailwind)
- **`frontend/src/App.tsx`** - Compact frameless overlay UI. Renders state received from Go events: recording (pulsing dot + elapsed time), processing (spinner), "Copied!" confirmation, error display.
- **`frontend/src/hooks/use-voicebox.ts`** - Listens for `voicebox:state` events from Go backend and provides UI state. No audio/WebSocket/clipboard logic — purely a display layer.

### Cloud Backend
- **`worker/`** - Cloudflare Worker with Durable Object. WebSocket endpoint at `/ws` that accumulates PCM audio, wraps as WAV, runs Whisper → LLM, returns formatted text.

## Data Flow

1. User presses hotkey → Go hotkey handler fires `onHotkeyDown`
2. Go starts native audio capture (malgo/miniaudio) → PCM chunks flow to channel
3. Go opens WebSocket to Worker, streams chunks as they arrive
4. Go shows Wails overlay window, emits `voicebox:state` recording event to frontend
5. User releases hotkey → Go `onHotkeyUp` stops capture, closes channel
6. Pipeline sends `audio_end`, waits for server to transcribe + format
7. Go copies result to clipboard (pbcopy), emits copied state, hides window

## Worker WebSocket Protocol

Client connects to `GET /ws` with auth token and audio config via query params (`token`, `sampleRate`, `channels`, `encoding`).

- Client sends binary PCM chunks, then `{"type":"audio_end"}` when done
- Server sends `{"type":"ready"}`, `{"type":"processing","stage":"stt"|"format"}`, then `{"type":"result","raw":"...","formatted":"..."}`

## Window Behavior

Window is hidden by default. On recording start (hotkey or menu), Go shows a compact floating overlay (always-on-top). After result is copied to clipboard, a brief "Copied!" confirmation shows, then the window auto-hides.

## Key Dependencies

### Go
- `github.com/wailsapp/wails/v2` - desktop app framework (WebView + Go)
- `github.com/gen2brain/malgo` - miniaudio Go bindings for native audio capture
- `github.com/gorilla/websocket` - WebSocket client for Worker communication
- `github.com/BurntSushi/toml` - config parsing
- `golang.design/x/hotkey` - global hotkey registration

### Frontend
- React, Tailwind CSS v4, Vite
- `@wailsapp/runtime` - Wails JS runtime (events only, no bindings used)

### Worker
- Cloudflare Workers AI (`@cf/openai/whisper-large-v3-turbo`, `@cf/ibm-granite/granite-4.0-h-micro`)
- Durable Objects with hibernation WebSocket API

## Conventions

- Wails v2 menu keys API: `keys.Combo("r", keys.ControlKey, keys.ShiftKey)`, `keys.CmdOrCtrl("q")` — not `keys.CombKey`
- Wails v2 has no native system tray support
- Go: standard library preferred where possible
- TypeScript in `worker/`: Cloudflare Workers patterns, typed with `@cloudflare/workers-types`
- TypeScript in `frontend/`: React + Tailwind, functional components, hooks pattern
- Config file: `voicebox.toml` (gitignored, contains API tokens)
- Provider pattern: interfaces in `internal/stt/` and `internal/formatter/`, with cloud/local implementations
- Per-recording WebSocket: fresh connection per recording session, no persistent connections
- Audio: 16kHz, mono, PCM signed 16-bit LE, 4096-byte chunks
- Clipboard: uses `pbcopy` (macOS), cross-platform support can be added later
