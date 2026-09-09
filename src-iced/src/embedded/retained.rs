use std::cell::RefCell;
use std::rc::Rc;

use iced::advanced::graphics::{self, compositor, Shell, Viewport};
use iced::advanced::renderer;
use iced::{Color, Renderer};

use super::compositor::{Compositor, Surface};

/// The daemon factory retains a clone while iced drops its last window lease.
/// This keeps mpv, its device and enabled feature chain alive until daemon exit.
#[derive(Clone)]
pub(crate) struct RetainedCompositor(Rc<RefCell<Compositor>>);

impl graphics::Compositor for RetainedCompositor {
  type Renderer = Renderer;
  type Surface = Surface;

  async fn new(
    settings: graphics::core::backend::Settings,
    display: impl compositor::Display + Clone,
    window: impl compositor::Window + Clone,
    shell: Shell,
  ) -> Result<Self, graphics::core::backend::Error> {
    Compositor::new(settings, display, window, shell)
      .await
      .map(|compositor| Self(Rc::new(RefCell::new(compositor))))
  }

  fn create_renderer(&self, settings: renderer::Settings) -> Renderer {
    self.0.borrow().create_renderer(settings)
  }

  fn create_surface(
    &mut self,
    window: impl compositor::Window + Clone,
    width: u32,
    height: u32,
  ) -> Surface {
    self.0.borrow_mut().create_surface(window, width, height)
  }

  fn configure_surface(&mut self, surface: &mut Surface, width: u32, height: u32) {
    self
      .0
      .borrow_mut()
      .configure_surface(surface, width, height);
  }

  fn information(&self) -> compositor::Information {
    self.0.borrow().information()
  }

  fn present(
    &mut self,
    renderer: &mut Renderer,
    surface: &mut Surface,
    viewport: &Viewport,
    background: Color,
    on_pre_present: impl FnOnce(),
  ) -> Result<(), compositor::SurfaceError> {
    self
      .0
      .borrow_mut()
      .present(renderer, surface, viewport, background, on_pre_present)
  }

  fn screenshot(
    &mut self,
    renderer: &mut Renderer,
    viewport: &Viewport,
    background: Color,
  ) -> Vec<u8> {
    self
      .0
      .borrow_mut()
      .screenshot(renderer, viewport, background)
  }
}
