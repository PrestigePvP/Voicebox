## Overview

VoiceBox is a voice-to-text pipeline with a Wails v2 desktop app (Go + React frontend) and a Cloudflare Worker cloud backend. Press a hotkey to record speech. Audio is captured natively in Go via miniaudio, streamed via Go WebSocket client to a Cloudflare Worker that runs Whisper STT then LLM formatting. The result is copied to clipboard and auto-pasted into the originating app.

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
- **`main.go`** - Wails entrypoint. Configures window (700×450, frameless, visible on start), app menu, and binds App struct.
- **`app.go`** - App struct with `startup`/`shutdown` lifecycle, hotkey-driven recording orchestration, audio capture, WebSocket pipeline, clipboard copy, auto-paste, and window mode switching.
- **`window_darwin.go`** / **`window_other.go`** - Platform-specific window management: overlay (top-center, 160×48, floating), settings (centered, 700×450, normal level), dock click handler.
- **`internal/audio/`** - Audio capture using `malgo` (miniaudio Go bindings). Records PCM s16le at configured sample rate. Emits fixed-size chunks via channel. Also emits RMS level via callback.
- **`internal/pipeline/`** - Go WebSocket client. Connects to Worker, sends `configure` (audio params + focus context), streams PCM chunks, sends `audio_end`, receives transcription result.
- **`internal/hotkey/`** - `ParseHotkey` and `RegisterHotkey` using `golang.design/x/hotkey` for system-wide hotkey registration.
- **`internal/config/`** - TOML config loading with defaults. Searches `~/.config/voicebox/voicebox.toml`, then next to binary, then `./voicebox.toml`. Also supports `Save`.
- **`internal/accessibility/`** - macOS AX API + CGEvent: captures focused element context (app name, bundle ID, PID, role, title, placeholder, value) before recording starts; `PasteIntoApp` reactivates the app and simulates Cmd+V.
- **`internal/stt/`** - STT provider interface + stubs (CloudProvider, LocalProvider).
- **`internal/formatter/`** - Formatter provider interface + stubs (CloudProvider, LocalProvider).

### Frontend (React + TypeScript + Tailwind)
- **`frontend/src/App.tsx`** - Top-level component. Routes between two modes: `"settings"` (full settings UI) and `"overlay"` (compact recording widget).
- **`frontend/src/components/settings-form.tsx`** - Settings form (react-hook-form + zod). Reads/writes config via `GetConfig`/`SaveConfig`/`GetConfigPath` Wails bindings.
- **`frontend/src/components/title-bar.tsx`** - Frameless title bar with drag region and window controls.
- **`frontend/src/hooks/use-voicebox.ts`** - Listens for `voicebox:state`, `voicebox:mode`, and `voicebox:level` events from Go. Provides `uiState`, `mode`, and `level` to the UI.
- **`frontend/src/hooks/use-config.ts`** - Calls `GetConfig`/`SaveConfig`/`GetConfigPath` Wails bindings to load and persist config.

### Cloud Backend
- **`worker/`** - Cloudflare Worker with Durable Object. WebSocket endpoint at `/ws` that accumulates PCM audio, wraps as WAV, runs Whisper → LLM, returns formatted text.

## Data Flow

1. User presses hotkey → Go captures focus context via AX API (`GetFocusContext`)
2. Go starts native audio capture (malgo/miniaudio) → PCM chunks + RMS level flow to channels
3. Go switches window to overlay mode (160×48, top-center), emits `voicebox:mode` → `"overlay"` and `voicebox:state` → `"recording"`
4. Go opens WebSocket to Worker, sends `configure` (audio params + focus context), streams PCM chunks
5. Go emits `voicebox:level` each chunk for the VoiceMeter UI
6. User releases hotkey → Go `onHotkeyUp` stops capture, closes channel, emits `"processing"` state
7. Pipeline sends `audio_end`, waits for server to transcribe + format
8. Go copies result to clipboard (`pbcopy`), then calls `PasteIntoApp(pid)` to simulate Cmd+V in the original app
9. Go emits `"copied"` state, waits 1.5s, hides window, emits `"idle"`

## Worker WebSocket Protocol

Client connects to `GET /ws` with `Authorization: Bearer <token>` header.

- Server sends `{"type":"ready"}`
- Client sends `{"type":"configure","audio":{...},"context":{...}}`
- Client sends binary PCM chunks, then `{"type":"audio_end"}`
- Server sends `{"type":"processing","stage":"stt"|"format"}`, then `{"type":"result","raw":"...","formatted":"..."}`

## Window Behavior

App starts visible in settings mode (700×450, centered). On recording start, window switches to overlay mode (160×48, top-center, floating). After result is pasted, the overlay hides and returns to hidden state (reopened via dock click or menu).

Settings can be opened via:
- App menu > Recording > Show Settings
- Clicking the dock icon (when window is hidden)

## Key Dependencies

### Go
- `github.com/wailsapp/wails/v2` - desktop app framework (WebView + Go)
- `github.com/gen2brain/malgo` - miniaudio Go bindings for native audio capture
- `github.com/gorilla/websocket` - WebSocket client for Worker communication
- `github.com/BurntSushi/toml` - config parsing
- `golang.design/x/hotkey` - global hotkey registration

### Frontend
- React, Tailwind CSS v4, Vite
- react-hook-form, zod, @hookform/resolvers
- `@wailsapp/runtime` - Wails JS runtime (events + bindings)

### Worker
- Cloudflare Workers AI (`@cf/openai/whisper-large-v3-turbo`, `@cf/qwen/qwen3-30b-a3b-fp8`)
- Durable Objects with hibernation WebSocket API

## Conventions

- Wails v2 menu keys API: `keys.Combo("r", keys.ControlKey, keys.OptionOrAltKey)`, `keys.CmdOrCtrl("q")` — not `keys.CombKey`
- Wails v2 has no native system tray support
- Go: standard library preferred where possible
- TypeScript in `worker/`: Cloudflare Workers patterns, typed with `@cloudflare/workers-types`
- TypeScript in `frontend/`: React + Tailwind v4, functional components, hooks pattern
- Config file: `~/.config/voicebox/voicebox.toml` (primary), also checked next to binary and at `./voicebox.toml`
- Provider pattern: interfaces in `internal/stt/` and `internal/formatter/`, with cloud/local implementations
- Per-recording WebSocket: fresh connection per recording session, no persistent connections
- Audio: 16kHz, mono, PCM signed 16-bit LE, 4096-byte chunks
- Clipboard: uses `pbcopy` (macOS); auto-paste via CGEvent Cmd+V simulation
- macOS accessibility permission required for full focus context capture and auto-paste
