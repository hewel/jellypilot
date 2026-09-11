//! Executable-only unsafe integration boundary; no public library or factory exports.
#![deny(unsafe_op_in_unsafe_fn)]
// Desktop app: no console window on Windows.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

// Avoid glibc per-thread arenas retaining large idle reservations.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// Pumps this thread's Win32 message queue. Runs on the tray worker thread,
/// which owns the tray icon's hidden window; without it right-clicks never
/// reach `tray-icon`'s window procedure and the menu cannot open.
#[cfg(target_os = "windows")]
fn pump_thread_messages() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
    };
    // SAFETY: All calls operate on the calling thread's own message queue.
    // `msg` is a valid out-pointer; a null HWND targets every window owned by
    // this thread. No handles outlive the call.
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

#[cfg(target_os = "linux")]
fn application_engine(
    context: &jellypilot_mpv_host::DeviceContext,
    antialiasing: Option<iced_wgpu::graphics::Antialiasing>,
    shell: iced_wgpu::graphics::Shell,
) -> iced_wgpu::Engine {
    // SAFETY: This private function is passed only to the fixed JellyPilot runner,
    // which never returns its Engine/Renderer or accepts caller-defined widgets.
    // Its only custom GPU primitive is embedded::video::VideoPrimitive: prepare
    // copies a BindGroup and requests layout redraw; draw only records sampling
    // into iced's pass. It never submits, retains or exports device/queue handles.
    // All iced submits (including image workers) use the installed shared gate;
    // the app compositor guards surface acquire/configure/present separately.
    // Host owns the independent three-buffer copy transaction and retains context
    // through terminate and completion. No queue guard surrounds mpv or polling.
    // Adding another GPU primitive or exposing renderer callbacks requires a fresh
    // safety review here; this function is not a reusable public safe constructor.
    unsafe { context.engine(antialiasing, shell) }
}

#[cfg(target_os = "linux")]
fn configure_surface(
    context: &jellypilot_mpv_host::DeviceContext,
    surface: &iced_wgpu::wgpu::Surface<'_>,
    configuration: &iced_wgpu::wgpu::SurfaceConfiguration,
) {
    // SAFETY: Only the fixed app compositor receives this private callback. It
    // retains the surface privately and gates every acquire/present/discard.
    unsafe { context.configure_surface(surface, configuration) }
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    #[cfg(target_os = "windows")]
    jellypilot_iced::set_tray_message_pump(pump_thread_messages);
    #[cfg(target_os = "linux")]
    let factory = jellypilot_iced::EmbeddedEngineFactory {
        create_engine: application_engine,
        configure_surface,
    };
    #[cfg(not(target_os = "linux"))]
    let factory = ();
    jellypilot_iced::run(factory)
}
