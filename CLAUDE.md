## Overview

VoiceBox is a voice-to-text pipeline with a Tauri v2 desktop app (Rust + React frontend) and a Cloudflare Worker cloud backend. Press a hotkey to record speech. Audio is captured natively in Rust via cpal, streamed via tokio-tungstenite WebSocket client to a Cloudflare Worker that runs Whisper STT then LLM formatting. The result is copied to clipboard and auto-pasted into the originating app.

Local backend support (faster-whisper + Ollama) is available via the `server/` directory (separate Go binary).

**Meeting Mode** records in-person meetings (30–120 min) from the mic to a crash-safe local WAV, then batch-processes through the Worker: R2 multipart upload → `@cf/deepgram/nova-3` with diarization (single-request full-file — consistent speaker labels) → LLM name-inference/title/summary. Results land in `~/Documents/VoiceBox Meetings/<id>/` as `audio.wav` + `meeting.json` + `meeting.md`, viewable/renamable in the dedicated `meetings` window. Started from the tray or meetings window (no hotkey). Cloud-only: always uses `config.cloud` regardless of `provider.mode`.

## Commands

### Tauri (root directory)
- `cargo tauri dev` - run dev server (Rust + Vite hot reload)
- `cargo tauri build` - build standalone `.app` bundle
- `./scripts/install.sh` - build signed + install to `/Applications` (see Install & Autostart)
- `cargo clippy` - lint Rust code (run from `src-tauri/`)
- `cargo test` - run Rust tests (run from `src-tauri/`)
- `voicebox --meeting-file recording.wav` - headless: run an existing WAV through the full Meeting Mode pipeline (upload/diarize/enrich). `VOICEBOX_CONFIG=/path/to/config.json` overrides the config search path, useful for pointing at a `wrangler dev` worker.

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
- **`meeting.rs`** - Meeting Mode lifecycle: `Phase` state machine (Idle/Recording/Processing) in its own managed `MeetingState`, start/stop (tray + IPC), `run_processing` pipeline, `voicebox:meeting-state` events, and all meeting IPC commands. Blocks the dictation hotkey while recording (and vice versa).
- **`meeting_api.rs`** - reqwest client for the Worker's `/meetings/*` HTTP API: multipart upload (uniform 10 MiB parts, 3 retries each), transcribe (15 min timeout), enrich (best-effort — never fails the meeting).
- **`meeting_store.rs`** - Meeting folder management, `meeting.json` schema (camelCase, versioned), markdown rendering, list/load/save.
- **`wav_writer.rs`** - Incremental WAV writer; header patched + fsynced every ~5 s so the file is playable even after a crash mid-recording.
- **`hotkey.rs`** - Modifier-only hotkey via CGEventTap (macOS). Parses combo strings ("ctrl+cmd") to modifier bitmask. Runs CFRunLoop on dedicated thread. Press/release callbacks for hold-to-record.
- **`accessibility.rs`** - macOS AX API via raw objc runtime: captures focused element context (app name, bundle ID, PID, role, title, placeholder, value). CGEvent Cmd+V simulation for auto-paste.

### Frontend (React + TypeScript + Tailwind)
- **`frontend/src/App.tsx`** - Top-level component. Uses window label to route: `"main"` → settings UI, `"overlay"` → compact recording widget.
- **`frontend/src/components/settings-form.tsx`** - Settings form (react-hook-form + zod). Reads/writes config via Tauri `invoke`.
- **`frontend/src/components/title-bar.tsx`** - Frameless title bar with drag region (`data-tauri-drag-region`) and close button.
- **`frontend/src/hooks/use-voicebox.ts`** - Listens for `voicebox:state` and `voicebox:level` events from Rust via `@tauri-apps/api/event`. Provides `uiState` and `level` to the UI.
- **`frontend/src/hooks/use-config.ts`** - Calls `get_config`/`save_config`/`get_config_path` via Tauri `invoke`.
- **`frontend/src/hooks/use-meeting.ts`** - Listens for `voicebox:meeting-state` + `voicebox:level`; pulls initial state via `get_meeting_state`; exposes start/stop/retry.
- **`frontend/src/components/meetings/`** - Meetings window: `meetings-app.tsx` shell, `record-panel.tsx` (start/stop, elapsed, level, progress), `meeting-list.tsx`, `meeting-detail.tsx` (transcript, speaker legend with inline rename).

### Cloud Backend
- **`worker/`** - Cloudflare Worker with Durable Object. WebSocket endpoint at `/ws` that accumulates PCM audio, wraps as WAV, runs Whisper → LLM, returns formatted text.
- **`worker/src/meetings.ts`** - Meeting Mode HTTP routes (see Meeting HTTP API below). `diarize.ts` merges nova-3 utterances into speaker turns (8 s gap break, per-word fallback); `meeting-prompt.ts` samples long transcripts (~12k-word budget, front-weighted) and parses the strict-JSON enrich response with a never-fail fallback.

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

## Meeting HTTP API (`/meetings/*`, same bearer token)

- `POST /meetings/uploads` `{sizeBytes}` → `{key, uploadId}` (413 over 300 MB)
- `PUT /meetings/uploads/part?key&uploadId&partNumber` (raw bytes; parts must be uniform size except the last, ≥5 MiB) → `{partNumber, etag}`
- `POST /meetings/uploads/complete` `{key, uploadId, parts}` → `{sizeBytes}`
- `POST /meetings/transcribe` `{key}` → `{durationSec, model, speakerCount, turns:[{start,end,speaker,text}]}` — streams the R2 object into nova-3 (`diarize/punctuate/smart_format/utterances`). Workers AI omits `metadata.duration`; duration comes from the last utterance.
- `POST /meetings/enrich` `{turns}` → `{title, summary, speakers:{id:name|null}, model}` — uses `MEETING_ENRICH_MODEL` (llama-3.3-70b; the account cannot access gemma-3-12b-it, error 5018). Parse failures return the null-fallback shape with 200, never a 5xx.

R2 bucket `voicebox-meetings` has lifecycle rules: objects deleted after 2 days, incomplete multipart uploads aborted after 1 day. Meeting audio's source of truth is the local WAV, not R2.

## Install & Autostart

`./scripts/install.sh` builds with a local code-signing identity and replaces
`/Applications/VoiceBox.app`.

**Signing is not optional here.** Accessibility (CGEventTap hotkey, AX focus
context, Cmd+V paste) and Microphone TCC grants are keyed to the code
signature. Ad-hoc signing pins the cdhash, so every rebuild revokes both grants
and the app starts at login with a silently dead hotkey. A stable identity
gives a stable designated requirement, so the grants persist across rebuilds.

Setup (once per machine):
1. Keychain Access > Certificate Assistant > Create a Certificate — name
   `VoiceBox Local`, **Self Signed Root**, **Code Signing**.
2. `cp src-tauri/tauri.local.conf.example.json src-tauri/tauri.local.conf.json`
   and set `signingIdentity`. It is merged at build time via `--config`.
   Gitignored because the identity name is per-machine — **not** because it
   holds a secret; the private key lives in the keychain, never in the repo.

A self-signed root imports as untrusted, so `security find-identity -v` (valid
only) won't list it. `codesign` accepts it regardless, which is why
`install.sh` checks without `-v`. Don't "fix" this with `add-trusted-cert`:
trusting the root means trusting it to sign anything, for no gain here.

`hardenedRuntime: false` lives in the **local** config, not the committed one.
It is a self-signed-local concession — hardened runtime would force a mic
entitlements file for no benefit on a build that is never notarized. Anyone
building for distribution wants Tauri's default (`true`) plus an entitlements
file asserting `com.apple.security.device.audio-input`, so the committed
config must not pin it off.

`src-tauri/Info.plist` supplies `NSMicrophoneUsageDescription` (Tauri
auto-merges an `Info.plist` sitting next to `tauri.conf.json`). This is a TCC
requirement in its own right, independent of hardened runtime.

Autostart uses `tauri-plugin-autostart` with `MacosLauncher::LaunchAgent`,
passing `--autostart` at login. The plist is
`~/Library/LaunchAgents/VoiceBox.plist` — `auto-launch` names it from
`package_info().name` (i.e. `productName`), **not** the bundle identifier. The `main` window is declared `"visible": false` and
shown in `setup` only when that flag is absent — config windows are created
before `setup` runs, so hiding it there instead would race and flash on a cold
boot. The plugin's own state is the source of truth; nothing is mirrored into
`voicebox.json`, which would drift when the user removes the login item in
System Settings. Enabling requires `current_exe()` under `/Applications`
(the plist records whatever binary enabled it, so `cargo tauri dev` would pin
the dev binary), but an already-enabled login item stays switchable off from
anywhere — otherwise a stale plist is unreachable from the UI that made it.

## Window Behavior

Three windows managed by Tauri:
- **Settings window** (`"main"`): 700×450, frameless, centered, visible on start. Close hides (app stays in tray).
- **Overlay window** (`"overlay"`): transparent, always-on-top, no decorations, skip taskbar. Shown during recording, hidden after result. Shows a red-dot + elapsed pill during meeting recording.
- **Meetings window** (`"meetings"`): 900×650, resizable, frameless, hidden by default. Close hides. Opened via tray (`Meetings…`), the Meetings button in Settings, or automatically when a meeting starts from the tray.

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
