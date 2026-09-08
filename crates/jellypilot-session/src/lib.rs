//! Shared Jellyfin session protocol for JellyPilot frontends.

mod remote_control;
mod websocket;
pub use jellypilot_media_server::{GeneralCommand, JellyfinError, PlayRequest, PlaystateRequest};

pub use remote_control::{
    finalize_remote_target, remote_index_value, remote_state_after_event, remote_volume_value,
    CapabilityRegistrationError, RemoteControlState,
};
pub use websocket::{JellyfinCommand, JellyfinWebSocket, JellyfinWebSocketEvent};
