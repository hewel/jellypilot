//! Shared Jellyfin session protocol and Intro Skipper evaluation for JellyPilot frontends.

mod intro_skipper;
mod websocket;
pub use jellypilot_media_server::{GeneralCommand, JellyfinError, PlayRequest, PlaystateRequest};

pub use intro_skipper::{
    evaluate_intro_skip, evaluate_manual_skip, IntroSkipAction, IntroSkipDecision, IntroSkipKind,
    IntroSkipMode, IntroSkipRange,
};
pub use websocket::{JellyfinCommand, JellyfinWebSocket, JellyfinWebSocketEvent};
