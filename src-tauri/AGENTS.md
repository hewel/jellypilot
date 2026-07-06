# AGENTS.md — src-tauri

Rust backend for Tauri v2 desktop app. Controls external MPV player via JSON IPC.

## Where to Look

| Task | Location | Notes |
|------|----------|-------|
| Add command | `src/command.rs` | `#[tauri::command]` + `#[specta]` |
| Register command | `src/lib.rs` | Add to `collect_commands![]` macro |
| Change app config | `tauri.conf.json` | Window size, title, CSP, icons |
| Add Rust dependency | `cargo add` | Do not edit `Cargo.toml` directly |
| Build script | `build.rs` | Codegen |

## Conventions

- **All commands need `#[specta]`** for TypeScript binding generation
- **Entry via lib.rs**: main.rs just calls `app_lib::run()`

## Anti-Patterns

- **Forgetting collect_commands**: New commands must be registered in `collect_commands![]` in lib.rs

## Key Dependencies

- `tauri` v2.9 — Desktop app framework
- `tauri-specta` v2 — Type-safe Rust↔TS bindings
- `specta` + `specta-typescript` — Type generation
- `serde` + `serde_json` — Serialization
- `tauri-plugin-log` — Logging (debug builds)
- `tokio` — Async runtime
- `reqwest` + `tokio-tungstenite` — Jellyfin communication
- `tokio::net` — IPC with MPV
