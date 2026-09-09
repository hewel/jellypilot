use std::sync::Arc;

use iced::advanced::graphics::{self, compositor, Shell, Viewport};
use iced::advanced::renderer;
use iced::{Color, Renderer};
use iced_wgpu::{wgpu, Engine, QueueGuard};
use jellypilot_mpv_host::{DeviceContext, Host, HostOptions};

use super::video::{self, FRAME};

// Field order destroys mpv before its borrowed device and retained feature chain.
pub(crate) struct Compositor {
  host: Host,
  context: Arc<DeviceContext>,
  engine: Engine,
  instance: wgpu::Instance,
  binding_layout: wgpu::BindGroupLayout,
  generation: u64,
  integration: crate::EmbeddedEngineFactory,
}

pub(crate) struct Surface {
  native: Option<wgpu::Surface<'static>>,
  synchronization: Arc<jellypilot_mpv_host::QueueLock>,
}

impl Drop for Surface {
  fn drop(&mut self) {
    let _guard = QueueGuard::acquire(self.synchronization.as_ref());
    drop(self.native.take());
  }
}

struct AcquiredFrame {
  frame: Option<wgpu::SurfaceTexture>,
  synchronization: Arc<jellypilot_mpv_host::QueueLock>,
}

impl Drop for AcquiredFrame {
  fn drop(&mut self) {
    let _guard = QueueGuard::acquire(self.synchronization.as_ref());
    drop(self.frame.take());
  }
}

fn unavailable(reason: impl std::fmt::Display) -> iced::advanced::graphics::core::backend::Error {
  iced::advanced::graphics::core::backend::Error::GraphicsAdapterNotFound {
    backend: "JellyPilot embedded MPV",
    reason: iced::advanced::graphics::core::backend::Reason::RequestFailed(format!(
      "Embedded MPV unavailable: {reason}. Start with --external to use external MPV."
    )),
  }
}

impl graphics::Compositor for Compositor {
  type Renderer = Renderer;
  type Surface = Surface;

  async fn new(
    settings: iced::advanced::graphics::core::backend::Settings,
    display: impl compositor::Display + Clone,
    compatible_window: impl compositor::Window + Clone,
    shell: Shell,
  ) -> Result<Self, iced::advanced::graphics::core::backend::Error> {
    let options = super::options().ok_or_else(|| unavailable("not selected"))?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
      backends: wgpu::Backends::VULKAN,
      ..wgpu::InstanceDescriptor::new_with_display_handle(Box::new(display))
    });
    let surface = instance
      .create_surface(wgpu::SurfaceTarget::Window(Box::new(compatible_window)))
      .map_err(unavailable)?;
    let context = Arc::new(
      DeviceContext::new(&instance, &surface)
        .await
        .map_err(unavailable)?,
    );
    let engine = (options.engine_factory.create_engine)(
      &context,
      settings
        .antialiasing
        .then_some(graphics::Antialiasing::MSAAx4),
      shell.clone(),
    );
    let wake = shell.clone();
    let host = Host::new(
      context.clone(),
      HostOptions {
        libmpv: options.libmpv.clone(),
        baseline: options.baseline.clone(),
        ipc: options.ipc.clone(),
        width: 1,
        height: 1,
        extra_args: Vec::new(),
      },
      move || wake.request_redraw(),
    )
    .map_err(unavailable)?;
    let binding_layout = context.create_bind_group_layout(&video::layout_descriptor());
    let generation = host.frame_generation();
    {
      let mut frame = FRAME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
      frame.binding = Some(host.bind_group(&binding_layout));
      frame.layout = Some(binding_layout.clone());
      frame.shell = Some(shell);
    }
    Ok(Self {
      host,
      context,
      engine,
      instance,
      binding_layout,
      generation,
      integration: options.engine_factory,
    })
  }

  fn create_renderer(&self, settings: renderer::Settings) -> Renderer {
    Renderer::Primary(iced_wgpu::Renderer::new(self.engine.clone(), settings))
  }

  fn create_surface(
    &mut self,
    window: impl compositor::Window + Clone,
    width: u32,
    height: u32,
  ) -> Surface {
    let native = match self
      .instance
      .create_surface(wgpu::SurfaceTarget::Window(Box::new(window)))
    {
      Ok(surface)
        if surface
          .get_capabilities(self.context.adapter())
          .formats
          .contains(&wgpu::TextureFormat::Rgb10a2Unorm) =>
      {
        Some(surface)
      }
      Ok(_) => {
        tracing::error!(
          "Embedded MPV window does not support Rgb10a2Unorm; refusing 8-bit fallback"
        );
        None
      }
      Err(error) => {
        tracing::error!(%error, "Embedded MPV surface unavailable");
        None
      }
    };
    let mut surface = Surface {
      native,
      synchronization: self.context.queue_lock(),
    };
    if width > 0 && height > 0 {
      self.configure_surface(&mut surface, width, height);
    }
    surface
  }

  fn configure_surface(&mut self, surface: &mut Surface, width: u32, height: u32) {
    let Some(native) = &surface.native else {
      return;
    };
    if width == 0 || height == 0 {
      return;
    }
    (self.integration.configure_surface)(
      &self.context,
      native,
      &wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgb10a2Unorm,
        width,
        height,
        present_mode: wgpu::PresentMode::AutoVsync,
        alpha_mode: wgpu::CompositeAlphaMode::Opaque,
        view_formats: Vec::new(),
        desired_maximum_frame_latency: 1,
      },
    );
  }

  fn information(&self) -> compositor::Information {
    compositor::Information {
      adapter: self.context.adapter().get_info().name,
      backend: "Vulkan · embedded MPV · RGB10A2 SDR".into(),
    }
  }

  fn present(
    &mut self,
    renderer: &mut Renderer,
    surface: &mut Surface,
    viewport: &Viewport,
    background: Color,
    on_pre_present: impl FnOnce(),
  ) -> Result<(), compositor::SurfaceError> {
    let Renderer::Primary(renderer) = renderer else {
      tracing::error!("Embedded MPV refuses a software fallback renderer");
      return Err(compositor::SurfaceError::Other);
    };
    let native = surface
      .native
      .as_mut()
      .ok_or(compositor::SurfaceError::Other)?;
    let size = FRAME
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner)
      .size;
    // mpv API calls and fence polling must never run under the native queue gate.
    self.host.resize(size.0, size.1).map_err(|error| {
      tracing::error!(%error, "Embedded MPV resize failed");
      compositor::SurfaceError::Other
    })?;
    self.host.retry_if_capacity().map_err(|error| {
      tracing::error!(%error, "Embedded MPV redraw failed");
      compositor::SurfaceError::Other
    })?;
    self.host.copy_ready();
    if self.host.frame_generation() != self.generation {
      self.generation = self.host.frame_generation();
      FRAME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .binding = Some(self.host.bind_group(&self.binding_layout));
    }
    self.context.poll();
    let lock = self.context.queue_lock();
    let mut acquired = {
      let _guard = QueueGuard::acquire(lock.as_ref());
      match native.get_current_texture() {
        // A supported 10-bit swapchain can remain suboptimal on an 8-bit desktop.
        // Its image is valid; rejecting it forever prevents any video presentation.
        wgpu::CurrentSurfaceTexture::Success(frame)
        | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => AcquiredFrame {
          frame: Some(frame),
          synchronization: lock.clone(),
        },
        wgpu::CurrentSurfaceTexture::Outdated => return Err(compositor::SurfaceError::Outdated),
        wgpu::CurrentSurfaceTexture::Timeout => return Err(compositor::SurfaceError::Timeout),
        wgpu::CurrentSurfaceTexture::Occluded => return Err(compositor::SurfaceError::Occluded),
        wgpu::CurrentSurfaceTexture::Lost => return Err(compositor::SurfaceError::Lost),
        wgpu::CurrentSurfaceTexture::Validation => return Err(compositor::SurfaceError::Other),
      }
    };
    if let Some(frame) = &acquired.frame {
      let view = frame.texture.create_view(&Default::default());
      // Renderer owns the submit guard; an outer guard here would deadlock.
      renderer.present(
        Some(background),
        wgpu::TextureFormat::Rgb10a2Unorm,
        &view,
        viewport,
      );
    }
    on_pre_present();
    {
      let _guard = QueueGuard::acquire(lock.as_ref());
      if let Some(frame) = acquired.frame.take() {
        frame.present();
      }
    }
    Ok(())
  }

  fn screenshot(
    &mut self,
    renderer: &mut Renderer,
    viewport: &Viewport,
    background: Color,
  ) -> Vec<u8> {
    match renderer {
      Renderer::Primary(renderer) => renderer.screenshot(viewport, background),
      Renderer::Secondary(_) => unreachable!("embedded compositor only creates GPU renderers"),
    }
  }
}

impl Drop for Compositor {
  fn drop(&mut self) {
    let mut frame = FRAME
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    frame.binding = None;
    frame.layout = None;
    frame.shell = None;
  }
}
