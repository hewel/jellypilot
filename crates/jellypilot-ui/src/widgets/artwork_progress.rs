//! Playback strip clipped through the full artwork frame, not strip-sized corners.

use iced::advanced::{layout, renderer, widget, Layout, Renderer as _, Widget};
use iced::widget::{image, Image};
use iced::{border::Radius, Color, ContentFit, Element, Length};

#[derive(Debug, Clone, Copy)]
pub struct Style {
    pub fill: Color,
    pub track: Color,
    pub blur: f32,
}

pub struct ArtworkProgress {
    progress: f32,
    frame_height: f32,
    strip_height: f32,
    radius: Radius,
    artwork: Option<image::Handle>,
    style: Style,
}

impl ArtworkProgress {
    pub fn new(
        progress: f64,
        frame_height: f32,
        strip_height: f32,
        radius: Radius,
        artwork: Option<image::Handle>,
        style: Style,
    ) -> Self {
        Self {
            progress: if progress.is_finite() {
                (progress / 100.0).clamp(0.0, 1.0) as f32
            } else {
                0.0
            },
            frame_height,
            strip_height,
            radius,
            artwork,
            style,
        }
    }
}

impl<Message> Widget<Message, iced::Theme, iced::Renderer> for ArtworkProgress {
    fn size(&self) -> iced::Size<Length> {
        iced::Size::new(Length::Fill, Length::Fixed(self.strip_height))
    }

    fn layout(
        &mut self,
        _tree: &mut widget::Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, Length::Fill, Length::Fixed(self.strip_height))
    }

    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: iced::mouse::Cursor,
        viewport: &iced::Rectangle,
    ) {
        let strip = layout.bounds();
        let Some(visible) = strip.intersection(viewport) else {
            return;
        };
        let frame = iced::Rectangle {
            y: strip.y + strip.height - self.frame_height,
            height: self.frame_height,
            ..strip
        };
        let border = iced::Border {
            radius: self.radius,
            smoothing: super::container::SURFACE_SMOOTHING,
            ..iced::Border::default()
        };
        renderer.with_layer(visible, |renderer| {
            if self.style.blur > 0.0 {
                if let Some(handle) = &self.artwork {
                    let local_frame = iced::Rectangle {
                        x: 0.0,
                        y: strip.height - self.frame_height,
                        width: strip.width,
                        height: self.frame_height,
                    };
                    let image = Image::new(handle.clone())
                        .content_fit(ContentFit::Cover)
                        .display_frame(local_frame)
                        .mask_frame(local_frame)
                        .visible_region(iced::Rectangle::new(iced::Point::ORIGIN, strip.size()))
                        .border_radius(self.radius)
                        .border_smoothing(super::container::SURFACE_SMOOTHING)
                        .snap(false)
                        .blur(self.style.blur);
                    <Image as Widget<Message, iced::Theme, iced::Renderer>>::draw(
                        &image, tree, renderer, theme, style, layout, cursor, viewport,
                    );
                }
            }
            // WGPU must composite the tint after the filtered image.
            renderer.with_layer(visible, |renderer| {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: frame,
                        border,
                        snap: false,
                        ..renderer::Quad::default()
                    },
                    self.style.track,
                );
            });
            let watched = iced::Rectangle {
                width: strip.width * self.progress,
                ..strip
            };
            if let Some(watched) = watched.intersection(&visible) {
                renderer.with_layer(watched, |renderer| {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: frame,
                            border,
                            snap: false,
                            ..renderer::Quad::default()
                        },
                        self.style.fill,
                    );
                });
            }
        });
    }
}

impl<'a, Message: 'a> From<ArtworkProgress> for Element<'a, Message> {
    fn from(value: ArtworkProgress) -> Self {
        Self::new(value)
    }
}
