//! Cross-platform Vulkan / gpu-next host ABI boundary for embedded JellyPilot playback.
//!
//! Unsafe Vulkan and libmpv FFI is confined here. The private launcher performs
//! only the two documented unsafe handoffs into the closed application compositor.
//! Every unsafe operation remains explicit under `deny(unsafe_op_in_unsafe_fn)`.
//!
//! Platform differences are confined to decoder-import device extensions
//! (Linux DMA-BUF, Windows NT handles) and the IPC endpoint shape (Linux
//! filesystem socket, Windows named pipe).
//!
//! The enabled device feature chain is the exact retained creation chain, not
//! a supported-feature query. Hosts and pending GPU callbacks retain its context.
//! Three producer images are borrowed until mpv's scheduled release and the
//! subsequent private GPU copy complete. The copy is three ordered command buffers
//! (tracked COPY_DST transition, raw copy, tracked RESOURCE transition); no CPU
//! readback or color conversion is performed by this copy. Windows presentation
//! may use an eight-bit UNORM surface while the private texture remains ten-bit.
//!
//! The application must serialize all compositor queue operations with
//! `DeviceContext::queue_lock`; never hold it across Host methods or synchronous
//! mpv calls. Device and queue handles do not escape the closed integration;
//! the unsafe engine and surface handoffs document that obligation. Load only the
//! packaged, trusted fork implementing host ABI version 2 and its trusted baseline.
//! A version 1 build is accepted but keeps the fixed SDR target contract.
//! Dynamic loading is an executable-code trust boundary, not a sandbox. Playback controls
//! and errors use the existing JSON IPC client, not a second libmpv event model.
//!
//! mpv blocks for producer completion; this does not claim full-chain zero-copy,
//! hardware decoding, or downstream compositor presentation feedback. HDR10
//! presentation is available when the negotiated ABI and display chain allow it.
#![deny(unsafe_op_in_unsafe_fn)]
#![cfg(any(target_os = "linux", target_os = "windows"))]

mod gpu;
#[cfg(target_os = "linux")]
mod idle;
mod interop;

pub use gpu::DeviceContext;
pub use iced_wgpu::wgpu;
#[cfg(target_os = "linux")]
pub use idle::IdleInhibit;
pub use interop::{Host, QueueGuard, QueueLock, TargetColor};

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
    /// Application directory for temporary demuxer cache files, created at host startup.
    pub demuxer_cache_dir: std::path::PathBuf,
    pub width: u32,
    pub height: u32,
    /// Strict `--name=value` playback options. Device, VO, config, scripts,
    /// profiles, input bindings and IPC overrides are rejected, including aliases.
    pub extra_args: Vec<String>,
}

impl HostOptions {
    /// Checks paths and option policy without loading a library or allocating GPU resources.
    pub fn validate(&self) -> Result<(), Error> {
        for path in [&self.libmpv, &self.baseline, &self.demuxer_cache_dir] {
            if !path.is_absolute() || path.to_str().is_none_or(|value| value.contains('\0')) {
                return Err("embedded mpv paths must be absolute, UTF-8 and NUL-free".into());
            }
        }
        ipc_endpoint_valid(&self.ipc)?;
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

/// The IPC endpoint is a filesystem socket path on Linux and a named-pipe
/// path on Windows; both must be UTF-8 and NUL-free.
#[cfg(target_os = "linux")]
fn ipc_endpoint_valid(path: &std::path::Path) -> Result<(), Error> {
    if !path.is_absolute() || path.to_str().is_none_or(|value| value.contains('\0')) {
        return Err(
            "embedded mpv IPC endpoint must be an absolute, UTF-8, NUL-free socket path".into(),
        );
    }
    Ok(())
}

/// mpv's `--input-ipc-server` on Windows takes a `\\.\pipe\` name, not a file.
#[cfg(target_os = "windows")]
fn ipc_endpoint_valid(path: &std::path::Path) -> Result<(), Error> {
    const PIPE_PREFIX: &str = r"\\.\pipe\";
    let Some(value) = path.to_str() else {
        return Err("embedded mpv IPC endpoint must be a UTF-8 named-pipe path".into());
    };
    let valid = value
        .get(..PIPE_PREFIX.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(PIPE_PREFIX))
        && value.len() > PIPE_PREFIX.len()
        && !value.contains('\0');
    if !valid {
        return Err(
            "embedded mpv IPC endpoint must be a non-empty, NUL-free \\\\.\\pipe\\ name".into(),
        );
    }
    Ok(())
}
