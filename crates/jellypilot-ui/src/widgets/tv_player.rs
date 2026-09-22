//! TV cinema surfaces and controls; no desktop-player appearance is changed.

use iced::advanced::{layout, renderer, widget, Layout, Renderer as _, Widget};
use iced::widget::{button, container, space};
use iced::{Alignment, Background, Border, Color, Element, Length, Rectangle, Size, Theme};

use crate::tv;

pub fn control(
    focus: f32,
    primary: bool,
    round: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let disabled = status == button::Status::Disabled;
        let focus = if disabled { 0.0 } else { focus };
        let background = if primary {
            Color::WHITE
        } else {
            Color::from_rgba(0.0, 0.0, 0.0, if disabled { 0.25 } else { 0.6 })
        };
        button::Style {
            background: Some(Background::Color(super::motion::lerp_color(
                background,
                tv::FOCUS_SURFACE,
                focus,
            ))),
            text_color: foreground(focus, primary, !disabled),
            border: Border {
                radius: if round { 99.0.into() } else { 12.0.into() },
                ..Border::default()
            },
            ..button::Style::default()
        }
    }
}

pub fn foreground(focus: f32, primary: bool, enabled: bool) -> Color {
    if !enabled {
        return tv::PALETTE.text.muted;
    }
    if primary {
        return tv::ON_FOCUS;
    }
    super::motion::lerp_color(tv::PALETTE.text.heading, tv::ON_FOCUS, focus)
}

pub fn supporting(focus: f32) -> Color {
    super::motion::lerp_color(tv::PALETTE.text.body, tv::ON_FOCUS_SECONDARY, focus)
}

/// Selected speed retains its indigo edge when the light focus fill replaces its surface.
pub fn choice(
    focus: f32,
    selected: bool,
    speed: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let mut style = tv::button_progress(tv::PALETTE, focus, selected)(theme, status);
        if speed && selected {
            style.border.color = tv::PALETTE.colors.secondary;
            style.border.width = 2.0;
        }
        style
    }
}

pub fn panel(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgb8(0x15, 0x16, 0x1c).into()),
        text_color: Some(tv::PALETTE.text.heading),
        border: Border {
            radius: 16.0.into(),
            ..Border::default()
        },
        ..container::Style::default()
    }
}

pub fn session_switch<'a, Message: 'a>(selected: bool, scale: f32) -> Element<'a, Message> {
    let thumb = container(space())
        .width(24.0 * scale)
        .height(24.0 * scale)
        .style(|_| container::Style {
            background: Some(Color::WHITE.into()),
            border: Border::default().rounded(99),
            ..container::Style::default()
        });
    container(thumb)
        .width(56.0 * scale)
        .height(32.0 * scale)
        .padding(4.0 * scale)
        .align_x(if selected {
            Alignment::End
        } else {
            Alignment::Start
        })
        .style(move |_| container::Style {
            background: Some(
                if selected {
                    tv::PALETTE.colors.primary
                } else {
                    tv::PALETTE.colors.outline
                }
                .into(),
            ),
            border: Border::default().rounded(99),
            ..container::Style::default()
        })
        .into()
}

/// Localized seek emphasis; the fixed 30px slot does not move neighboring controls.
pub fn timeline<'a, Message: 'a>(
    position: f64,
    duration: f64,
    buffered: &'a [(f64, f64)],
    focus: f32,
    scale: f32,
) -> Element<'a, Message> {
    Element::new(Timeline {
        position,
        duration,
        buffered,
        focus,
        scale,
    })
}

struct Timeline<'a> {
    position: f64,
    duration: f64,
    buffered: &'a [(f64, f64)],
    focus: f32,
    scale: f32,
}

impl<Message> Widget<Message, Theme, iced::Renderer> for Timeline<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(30.0 * self.scale))
    }
    fn layout(
        &mut self,
        _: &mut widget::Tree,
        _: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, Length::Fill, 30.0 * self.scale)
    }
    fn draw(
        &self,
        _: &widget::Tree,
        renderer: &mut iced::Renderer,
        _: &Theme,
        _: &renderer::Style,
        layout: Layout<'_>,
        _: iced::mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        if bounds.width <= 0.0 || !self.duration.is_finite() || self.duration <= 0.0 {
            return;
        }
        let height = (6.0 + 2.0 * self.focus) * self.scale;
        let rail = Rectangle {
            x: bounds.x,
            y: bounds.center_y() - height / 2.0,
            width: bounds.width,
            height,
        };
        let fraction = if self.position.is_finite() {
            (self.position / self.duration).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let thumb = (16.0 + 8.0 * self.focus) * self.scale;
        renderer.with_layer(*viewport, |renderer| {
            let mut fill = |bounds: Rectangle, color: Color| {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds,
                        border: Border::default().rounded(99),
                        ..renderer::Quad::default()
                    },
                    color,
                )
            };
            fill(rail, Color::from_rgba(1.0, 1.0, 1.0, 0.2));
            for &(start, end) in self.buffered {
                if !start.is_finite() || !end.is_finite() || end <= start {
                    continue;
                }
                let left = (start / self.duration).clamp(0.0, 1.0) as f32;
                let right = (end / self.duration).clamp(0.0, 1.0) as f32;
                fill(
                    Rectangle {
                        x: rail.x + left * rail.width,
                        width: (right - left) * rail.width,
                        ..rail
                    },
                    Color::from_rgba(1.0, 1.0, 1.0, 0.4),
                );
            }
            fill(
                Rectangle {
                    width: fraction * rail.width,
                    ..rail
                },
                tv::PALETTE.colors.primary,
            );
            fill(
                Rectangle {
                    x: rail.x + fraction * rail.width - thumb / 2.0,
                    y: bounds.center_y() - thumb / 2.0,
                    width: thumb,
                    height: thumb,
                },
                super::motion::lerp_color(Color::WHITE, tv::FOCUS_SURFACE, self.focus),
            );
        });
    }
}
