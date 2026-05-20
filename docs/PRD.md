# Product Requirements Document (PRD): Jellyfin MPV Shim (Rust Refactor)

## 1. Project Overview

**Name:** `jellyfin-mpv-shim-rust`
**Goal:** Develop a high-performance, memory-safe, and stable replacement for the existing Python-based `jellyfin-mpv-shim`.
**Core Philosophy:**

1. **External Player Only:** The application must **not** embed `libmpv`. It acts strictly as a "Bridge/Controller" that spawns and controls a standalone `mpv` executable via JSON IPC. This ensures maximum video quality and compatibility with user custom configs (`mpv.conf`) and shaders.
2. **Type Safety:** Utilize `tauri-specta` to ensure 100% type-safe communication between the Rust backend and the TypeScript frontend.

## 2. Technology Stack

* **Language:** Rust (Latest Stable).
* **Code Formatting:** `rustfmt` configured with `tab_spaces = 2`.
* **GUI Framework:** Tauri v2 (Frontend: React/TypeScript + Tailwind CSS).
* **Type Sync:** `tauri-specta` v2 (with `specta-typescript`) for generating frontend bindings from Rust types.
* **Async Runtime:** Tokio.
* **Communication:**
* **Jellyfin:** `reqwest` (HTTP), `tokio-tungstenite` (WebSocket).
* **MPV:** `interprocess` (or raw `tokio::net`) for Named Pipes (Windows) / Unix Domain Sockets (Linux/macOS).
* **Serialization:** `serde`, `serde_json`.



## 3. Architecture Design

The system is composed of three distinct actors:

1. **The Sentinel (Rust/Tauri):** The main application daemon.
* Manages the GUI (Settings, Tray).
* Handles Service Discovery (UDP).
* Maintains the Jellyfin WebSocket session.
* Exposes strictly typed Commands/Events to the Frontend via `tauri-specta`.


2. **The Bridge (Rust IPC):**
* Spawns the external `mpv` process.
* Translates abstract commands (e.g., `Jellyfin::Play`) into MPV JSON IPC messages.
* Monitors MPV process health (auto-restart on crash).


3. **The Player (External Process):**
* A standard `mpv` installation on the host system.
* Controlled exclusively via `--input-ipc-server`.



## 4. Functional Requirements

### 4.1. MPV Process Management (The Bridge)

* **Detection:** Auto-detect `mpv` in `PATH` or common install locations. Allow user override in Settings.
* **Spawning:**
* Spawn `mpv` as a child process.
* **Mandatory Flags:**
* `--input-ipc-server=\\.\pipe\jellyfin-shim-socket` (Windows) or `/tmp/jellyfin-shim-socket` (Linux/macOS).
* `--idle` (Keep running without media).
* `--force-window` (Ensure window exists even for audio/idle).
* `--keep-open` (Don't close on EOF).




* **Lifecycle Management:**
* **Aggressive Cleanup:** Configurable option to terminate the MPV process completely when playback stops (to clear VRAM/Memory leaks) and respawn on the next play command.
* **Crash Recovery:** Detect unexpected process termination and auto-respawn or alert the user.



### 4.2. JSON IPC Core

* **Protocol:** Implement asynchronous JSON IPC over Named Pipes/Sockets.
* **Command Translation:**
* `Play(url)` -> `{ "command": ["loadfile", url] }`
* `Seek(time)` -> `{ "command": ["seek", time, "absolute"] }`
* `Pause(bool)` -> `{ "command": ["set_property", "pause", bool] }`
* `SetAudio(id)` -> `{ "command": ["set_property", "aid", id] }`
* `SetSubtitle(id)` -> `{ "command": ["set_property", "sid", id] }`


* **State Observation:**
* Subscribe to properties: `time-pos`, `pause`, `volume`, `mute`, `duration`, `track-list`.
* Debounce rapid changes before sending updates to Jellyfin.



### 4.3. Jellyfin Integration

* **Discovery:** Listen for UDP broadcasts to allow the Jellyfin server to discover the client.
* **Control:** Maintain a persistent WebSocket connection. Handle `Play`, `PlayState` (Seek/Pause), and `GeneralCommand` messages.
* **Reporting:** Send `ReportPlaybackProgress` to Jellyfin periodically with the state retrieved from MPV IPC.

### 4.4. Frontend & Type Safety (Tauri + Specta)

* **Configuration:** Use `tauri-specta` to export all Commands and Events to `bindings.ts`.
* **Commands (Rust -> TS):**
* `play_media(url: String)`
* `set_mpv_path(path: String)`
* `get_player_status()`


* **Events (Rust -> TS):**
* `PlayerStateChanged` (Payload: `{ time: number, paused: boolean, volume: number }`)
* `LogMessage` (Payload: String)


* **UI Features:**
* **Tray Icon:** Menu for "Show Settings", "Quit".
* **Settings Page:**
* MPV Executable Path selector.
* Jellyfin Server URL / API Key (manual entry fallback).
* "Always Transcode" toggle.
* **Shader Pack Selector:** A dropdown to select glsl-shader profiles (e.g., "Anime4K", "FSR").





## 5. Non-Functional Requirements

* **Stability:** The Shim must NOT crash if MPV crashes. It should simply log the error and wait for the next command.
* **Memory Usage:** Rust binary should use minimal RAM (<50MB). MPV memory usage is externalized.
* **Developer Experience:** All frontend-backend interfaces must be typed. No `any` or untyped `invoke` calls.

## 6. Implementation Phases

### Phase 1: The IPC Driver (Rust CLI)

* Create a Rust binary that spawns `mpv`.
* Implement the async IPC connection loop.
* Verify sending `loadfile` and receiving `time-pos` updates via `println!`.

### Phase 2: Jellyfin Protocol & State

* Implement Jellyfin WebSocket connection.
* Create the "Virtual Player" state machine in Rust that syncs Jellyfin commands to the Phase 1 IPC driver.

### Phase 3: Tauri Integration & Specta

* Initialize Tauri v2 project.
* Configure `tauri-specta` v2.
* Expose Phase 2 logic as Specta-decorated Commands.
* Generate `bindings.ts`.

### Phase 4: The Frontend

* Build the Settings UI using React.
* Implement System Tray logic.
* Finalize packaging (MSI/Deb/AppImage).
