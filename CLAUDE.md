## Overview

VoiceBox is a voice-to-text pipeline with a Tauri v2 desktop app (Rust + React frontend) and a Cloudflare Worker cloud backend. Press a hotkey to record speech. Audio is captured natively in Rust via cpal, streamed via tokio-tungstenite WebSocket client to a Cloudflare Worker that runs Whisper STT then LLM formatting. The result is copied to clipboard and auto-pasted into the originating app.

Local mode runs entirely on-device: Whisper inference in-process via `whisper-rs` (whisper.cpp with Metal), no network at all. Models are GGML `.bin` files downloaded in-app to `~/.config/voicebox/models/`.

The `server/` directory holds a standalone Go server (faster-whisper + Ollama) from an earlier design. **The desktop app no longer targets it** — it is retained but unreferenced.

## Commands

### Tauri (root directory)
- `cargo tauri dev` - run dev server (Rust + Vite hot reload)
- `cargo tauri build` - build standalone `.app` bundle

**`cargo build` succeeding does not mean `cargo tauri build` will.** The bundle build exports `MACOSX_DEPLOYMENT_TARGET`; the dev build sets nothing. whisper.cpp's ggml uses `std::filesystem::path`, which needs 10.15+. Two settings pin this and both are required — `bundle.macOS.minimumSystemVersion` in `tauri.conf.json`, and `MACOSX_DEPLOYMENT_TARGET = { value = "10.15", force = true }` in `src-tauri/.cargo/config.toml`. `force` is needed because Tauri sets the variable itself before invoking cargo, and a plain `[env]` entry never overrides an already-set variable. Changing either requires `rm -rf target/*/build/whisper-rs-sys-*` — `cargo clean -p whisper-rs-sys` leaves the stale `CMakeCache.txt` behind.

**`cargo tauri build` pops ~6 macOS crash-reporter windows.** These are cmake try-compile probes (`cmTC_*`) detecting ARM CPU features by running code and catching SIGILL. Benign, not the app crashing. Incremental builds and `cargo tauri dev` produce none.
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

### Local Server (`server/` directory) — orphaned
- `go run .` - run the standalone Go server. No longer used by the desktop app.

### Testing local mode without a mic or hotkey

Two headless flags on the app binary run the real config, model, and `local_engine::run`, so local mode can be exercised without Accessibility permission:

```bash
voicebox --transcribe-file audio.wav   # WAV -> transcript, fully scriptable
voicebox --record-test 9               # record N seconds from the mic -> transcript
```

Generate test audio: `say -o t.aiff "..." && ffmpeg -i t.aiff -ar 16000 -ac 1 -c:a pcm_s16le t.wav`

For a reproducible acoustic test, start `--record-test` and `afplay` a known WAV through the speakers — this covers cpal device selection, resampling, and chunking, which the WAV path skips.

## Architecture

### Rust Backend (`src-tauri/src/`)
- **`main.rs`** - Binary entrypoint, calls `lib::run()`.
- **`lib.rs`** - Tauri setup: app state management, tray icon, multi-window (settings + overlay), hotkey registration, recording orchestration (hotkey down/up → audio capture → pipeline → clipboard → paste), IPC commands (`get_config`, `save_config`, `get_config_path`).
- **`config.rs`** - JSON config load/save/defaults. Searches `~/.config/voicebox/voicebox.json`, then next to executable, then `./voicebox.json`. Auto-migrates existing TOML configs to JSON.
- **`audio.rs`** - Audio capture using `cpal`. Records PCM s16le at configured sample rate. Runs on dedicated thread (cpal::Stream is !Send). Emits fixed-size chunks via tokio mpsc channel. Emits RMS level via Tauri events (~30fps).
- **`pipeline.rs`** - Cloud mode only. Async WebSocket client via `tokio-tungstenite`. Connects to the Worker, sends `configure` (audio params + focus context), streams PCM chunks, sends `audio_end`, receives transcription result. Spawns sender/receiver tasks.
- **`local_engine.rs`** - Local mode. Drains PCM chunks, gates on silence/length (Whisper hallucinates on near-silent input), converts s16le→f32, runs whisper.cpp inference in `spawn_blocking` wrapped in `catch_unwind`, filters non-speech markers and subtitle-credit boilerplate, then optionally formats via Ollama. Formatter failure falls back to raw text rather than losing the transcript.
- **`models.rs`** - Whisper model catalog (id, size, SHA-256), download from Hugging Face to `~/.config/voicebox/models/` via `.partial` + streaming hash verify + atomic rename, plus list/delete. Progress events throttled to ~10/sec.
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
3. Rust shows overlay window, emits `voicebox:state` → `"recording"`
4. `run_pipeline` (`lib.rs`) branches on `provider.mode` — **the only place mode is read**:
   - `"cloud"` → `pipeline::run` opens a WebSocket and streams chunks as they arrive
   - `"local"` → `local_engine::run` accumulates chunks and transcribes on release
5. Rust emits `voicebox:level` each chunk for the VoiceMeter UI
6. User releases hotkey → Rust `on_hotkey_up` stops capture, closes channel, emits `"processing"` state
7. Cloud sends `audio_end` and awaits the server; local runs whisper.cpp locally
8. If the result is empty (silence), nothing is pasted — an empty paste would clobber the user's selection
9. Otherwise Rust copies to clipboard (`pbcopy`), then `paste_into_app(pid)` simulates Cmd+V
10. Rust emits `"copied"` state, waits 1.5s, hides overlay, emits `"idle"`

In local mode the Whisper context is loaded once and kept warm in `AppState`. Loading happens on a background thread — the first-ever Metal shader-library init takes several seconds and would otherwise stall app launch.

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
- `whisper-rs` (feature `metal`) - on-device Whisper via whisper.cpp, GPU-accelerated
- `reqwest` / `sha2` - model download and checksum verification
- `tokio-tungstenite` - async WebSocket client (cloud mode)
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
- **New config fields must be `#[serde(default)]`.** `config::load` treats a deserialize failure as "try the next path" and writes a fresh default if all fail — a required new field silently wipes the user's existing settings. See the tests in `config.rs`.
- Per-recording WebSocket (cloud mode): fresh connection per recording session, no persistent connections
- Audio: 16kHz, mono, PCM signed 16-bit LE, 4096-byte chunks
- Clipboard: uses `pbcopy` (macOS); auto-paste via CGEvent Cmd+V simulation
- macOS accessibility permission required for full focus context capture, hotkey, and auto-paste
