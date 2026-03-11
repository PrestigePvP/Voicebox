## Overview

VoiceBox is a voice-to-text pipeline with a Tauri v2 desktop app (Rust + React frontend) and a Cloudflare Worker cloud backend. Press a hotkey to record speech. Audio is captured natively in Rust via cpal, streamed via tokio-tungstenite WebSocket client to a Cloudflare Worker that runs Whisper STT then LLM formatting. The result is copied to clipboard and auto-pasted into the originating app.

Local backend support (faster-whisper + Ollama) is available via the `server/` directory (separate Go binary).

## Commands

### Tauri (root directory)
- `cargo tauri dev` - run dev server (Rust + Vite hot reload)
- `cargo tauri build` - build standalone `.app` bundle
- `cargo clippy` - lint Rust code (run from `src-tauri/`)
- `cargo test` - run Rust tests (run from `src-tauri/`)

### Frontend (`frontend/` directory)
- `pnpm install` - install dependencies
- `pnpm build` - production build
- `pnpm dev` - Vite dev server (used by `cargo tauri dev`)

### Cloudflare Worker (`worker/` directory)
- `pnpm install` - install dependencies
- `pnpm dev` - local dev server (wrangler)
- `pnpm deploy` - deploy to Cloudflare
- `pnpm lint` - type-check with tsc

### Local Server (`server/` directory)
- `go run .` - run local server (faster-whisper + Ollama)

## Architecture

### Rust Backend (`src-tauri/src/`)
- **`main.rs`** - Binary entrypoint, calls `lib::run()`.
- **`lib.rs`** - Tauri setup: app state management, tray icon, multi-window (settings + overlay), hotkey registration, recording orchestration (hotkey down/up → audio capture → pipeline → clipboard → paste), IPC commands (`get_config`, `save_config`, `get_config_path`).
- **`config.rs`** - JSON config load/save/defaults. Searches `~/.config/voicebox/voicebox.json`, then next to executable, then `./voicebox.json`. Auto-migrates existing TOML configs to JSON.
- **`audio.rs`** - Audio capture using `cpal`. Records PCM s16le at configured sample rate. Runs on dedicated thread (cpal::Stream is !Send). Emits fixed-size chunks via tokio mpsc channel. Emits RMS level via Tauri events (~30fps).
- **`pipeline.rs`** - Async WebSocket client via `tokio-tungstenite`. Connects to Worker/server, sends `configure` (audio params + focus context), streams PCM chunks, sends `audio_end`, receives transcription result. Spawns sender/receiver tasks.
- **`hotkey.rs`** - Modifier-only hotkey via CGEventTap (macOS). Parses combo strings ("ctrl+cmd") to modifier bitmask. Runs CFRunLoop on dedicated thread. Press/release callbacks for hold-to-record.
- **`accessibility.rs`** - macOS AX API via raw objc runtime: captures focused element context (app name, bundle ID, PID, role, title, placeholder, value). CGEvent Cmd+V simulation for auto-paste.

### Frontend (React + TypeScript + Tailwind)
- **`frontend/src/App.tsx`** - Top-level component. Uses window label to route: `"main"` → settings UI, `"overlay"` → compact recording widget.
- **`frontend/src/components/settings-form.tsx`** - Settings form (react-hook-form + zod). Reads/writes config via Tauri `invoke`.
- **`frontend/src/components/title-bar.tsx`** - Frameless title bar with drag region (`data-tauri-drag-region`) and close button.
- **`frontend/src/hooks/use-voicebox.ts`** - Listens for `voicebox:state` and `voicebox:level` events from Rust via `@tauri-apps/api/event`. Provides `uiState` and `level` to the UI.
- **`frontend/src/hooks/use-config.ts`** - Calls `get_config`/`save_config`/`get_config_path` via Tauri `invoke`.

### Cloud Backend
- **`worker/`** - Cloudflare Worker with Durable Object. WebSocket endpoint at `/ws` that accumulates PCM audio, wraps as WAV, runs Whisper → LLM, returns formatted text.

## Data Flow

1. User presses hotkey → Rust captures focus context via AX API (`get_focus_context`)
2. Rust starts native audio capture (cpal on dedicated thread) → PCM chunks + RMS level
3. Rust shows overlay window (160×48, top-center, always-on-top), emits `voicebox:state` → `"recording"`
4. Rust opens WebSocket to Worker, sends `configure` (audio params + focus context), streams PCM chunks
5. Rust emits `voicebox:level` each chunk for the VoiceMeter UI
6. User releases hotkey → Rust `on_hotkey_up` stops capture, closes channel, emits `"processing"` state
7. Pipeline sends `audio_end`, waits for server to transcribe + format
8. Rust copies result to clipboard (`pbcopy`), then calls `paste_into_app(pid)` to simulate Cmd+V
9. Rust emits `"copied"` state, waits 1.5s, hides overlay, emits `"idle"`

## Worker WebSocket Protocol

Client connects to `GET /ws` with `Authorization: Bearer <token>` header.

- Server sends `{"type":"ready"}`
- Client sends `{"type":"configure","audio":{...},"context":{...}}`
- Client sends binary PCM chunks, then `{"type":"audio_end"}`
- Server sends `{"type":"processing","stage":"stt"|"format"}`, then `{"type":"result","raw":"...","formatted":"..."}`

## Window Behavior

Two separate windows managed by Tauri:
- **Settings window** (`"main"`): 700×450, frameless, centered, visible on start. Close hides (app stays in tray).
- **Overlay window** (`"overlay"`): 160×48, transparent, always-on-top, no decorations, skip taskbar. Shown during recording, hidden after result.

Settings can be opened via:
- System tray icon click
- Tray menu > Show Settings

## Key Dependencies

### Rust
- `tauri` v2 - desktop app framework (multi-window, tray, IPC)
- `cpal` - cross-platform audio capture
- `tokio-tungstenite` - async WebSocket client
- `serde` / `serde_json` - config serialization
- `core-graphics` / `core-foundation` - macOS CGEventTap, CGEvent (hotkey + paste)
- `tauri-plugin-clipboard-manager` - clipboard access

### Frontend
- React, Tailwind CSS v4, Vite
- react-hook-form, zod, @hookform/resolvers
- `@tauri-apps/api` v2 - Tauri frontend IPC (events, invoke, window)

### Worker
- Cloudflare Workers AI (`@cf/openai/whisper-large-v3-turbo`)
- Durable Objects with hibernation WebSocket API

## Conventions

- TypeScript in `worker/`: Cloudflare Workers patterns, typed with `@cloudflare/workers-types`
- TypeScript in `frontend/`: React + Tailwind v4, functional components, hooks pattern
- Config file: `~/.config/voicebox/voicebox.json` (primary), also checked next to binary and at `./voicebox.json`
- Per-recording WebSocket: fresh connection per recording session, no persistent connections
- Audio: 16kHz, mono, PCM signed 16-bit LE, 4096-byte chunks
- Clipboard: uses `pbcopy` (macOS); auto-paste via CGEvent Cmd+V simulation
- macOS accessibility permission required for full focus context capture, hotkey, and auto-paste
