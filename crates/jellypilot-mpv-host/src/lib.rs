//! Linux Vulkan / gpu-next host ABI boundary for embedded JellyPilot playback.
//!
//! Unsafe Vulkan and libmpv FFI is confined here. The private launcher performs
//! only the two documented unsafe handoffs into the closed application compositor.
//! Every unsafe operation remains explicit under `deny(unsafe_op_in_unsafe_fn)`.
//!
//! The enabled device feature chain is the exact retained creation chain, not
//! a supported-feature query. Hosts and pending GPU callbacks retain its context.
//! Three producer images are borrowed until mpv's scheduled release and the
//! subsequent private GPU copy complete. The copy is three ordered command buffers
//! (tracked COPY_DST transition, raw copy, tracked RESOURCE transition); no CPU
//! readback, color conversion, or eight-bit fallback is performed.
//!
//! The application must serialize all compositor queue operations with
//! `DeviceContext::queue_lock`; never hold it across Host methods or synchronous
//! mpv calls. Device and queue handles do not escape the closed integration;
//! the unsafe engine and surface handoffs document that obligation. Load only the
//! packaged, trusted fork implementing host ABI version 1 and its trusted baseline.
//! Dynamic loading is an executable-code trust boundary, not a sandbox. Playback controls
//! and errors use the existing JSON IPC client, not a second libmpv event model.
//!
//! mpv blocks for producer completion; this does not claim full-chain zero-copy,
//! hardware decoding, HDR output, or downstream compositor presentation feedback.
#![deny(unsafe_op_in_unsafe_fn)]
#![cfg(target_os = "linux")]

mod gpu;
mod interop;

pub use gpu::DeviceContext;
pub use iced_wgpu::wgpu;
pub use interop::{Host, QueueGuard, QueueLock};

/// Sanitized initialization or GPU-host error; never includes media input.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Error(String);

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self(message.to_owned())
    }
}
impl From<String> for Error {
    fn from(message: String) -> Self {
        Self(message)
    }
}
macro_rules! gpu_error {
    ($($ty:ty),* $(,)?) => {$ (
        impl From<$ty> for Error {
            fn from(error: $ty) -> Self { Self(error.to_string()) }
        }
    )*};
}
gpu_error!(
    ash::vk::Result,
    wgpu::RequestAdapterError,
    wgpu::RequestDeviceError,
    wgpu::hal::DeviceError,
    std::num::TryFromIntError,
    std::ffi::NulError,
);
impl From<libloading::Error> for Error {
    fn from(_: libloading::Error) -> Self {
        Self("configured libmpv is missing required host ABI symbols".to_owned())
    }
}

/// Explicit assets and IPC endpoint. There is no system libmpv fallback.
#[derive(Debug, Clone)]
pub struct HostOptions {
    pub libmpv: std::path::PathBuf,
    pub baseline: std::path::PathBuf,
    pub ipc: std::path::PathBuf,
    pub width: u32,
    pub height: u32,
    /// Strict `--name=value` playback options. Device, VO, config, scripts,
    /// profiles, input bindings and IPC overrides are rejected, including aliases.
    pub extra_args: Vec<String>,
}

impl HostOptions {
    /// Checks paths and option policy without loading a library or allocating GPU resources.
    pub fn validate(&self) -> Result<(), Error> {
        for path in [&self.libmpv, &self.baseline, &self.ipc] {
            if !path.is_absolute() || path.to_str().is_none_or(|value| value.contains('\0')) {
                return Err("embedded mpv paths must be absolute, UTF-8 and NUL-free".into());
            }
        }
        if !self.libmpv.is_file() || !self.baseline.is_file() {
            return Err("embedded mpv library or baseline asset is missing".into());
        }
        if self.width == 0
            || self.height == 0
            || self.width > i32::MAX as u32
            || self.height > i32::MAX as u32
        {
            return Err("embedded mpv target dimensions must be positive C integers".into());
        }
        for argument in &self.extra_args {
            interop::parse_argument(argument)?;
        }
        Ok(())
    }
}
