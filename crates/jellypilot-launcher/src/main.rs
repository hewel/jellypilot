//! Executable-only unsafe integration boundary; no public library or factory exports.
#![deny(unsafe_op_in_unsafe_fn)]

// Avoid glibc per-thread arenas retaining large idle reservations.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

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
    #[cfg(target_os = "linux")]
    let factory = jellypilot_iced::EmbeddedEngineFactory {
        create_engine: application_engine,
        configure_surface,
    };
    #[cfg(not(target_os = "linux"))]
    let factory = ();
    jellypilot_iced::run(factory)
}
