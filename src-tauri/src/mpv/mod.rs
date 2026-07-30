//! MPV IPC module - spawns and controls external MPV player via JSON IPC.
//!
//! Architecture:
//! - `process.rs` - MPV binary detection and process spawning
//! - `ipc.rs` - Async IPC connection (Named Pipes on Windows, Unix Sockets on Linux/macOS)
//! - `protocol.rs` - JSON command/response types and serialization
//! - `client.rs` - High-level MPV client with command methods

mod client;
mod ipc;
mod process;
mod protocol;

pub(crate) use client::has_mpv_option;
pub use client::MpvClient;
pub use process::{find_mpv, write_input_conf};
pub use protocol::{MpvEvent, PropertyValue};
