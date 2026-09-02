//! External MPV process control, playback orchestration, and JSON IPC.

mod client;
mod config;
mod ipc;
pub mod playback;
pub mod playback_session;
pub mod player;
mod process;
mod protocol;
pub mod remote_commands;
mod state;

pub use client::{has_mpv_option, MpvClient, MpvError};
pub use config::configured_mpv_args;
pub use process::{find_mpv, jellypilot_input_conf_path, write_input_conf};
pub use protocol::{MpvEvent, PropertyValue};
pub use state::{
  collect_player_state, collect_player_state_sample, PlayerState, PlayerStateSample,
  TransportSnapshot,
};
