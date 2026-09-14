//! Cinema chrome, scoped to the embedded video surface.

use iced::advanced::{layout, renderer, widget, Layout, Renderer as _, Widget};
use iced::widget::{button, container, row, slider, space, text};
use iced::{
    gradient, Background, Border, Color, Degrees, Element, Length, Rectangle, Shadow, Size, Theme,
    Vector,
};

use crate::tokens::{DARK_PALETTE, TOKENS};
use crate::variants::ButtonVariant;

/// Black/60% fill and white/15% edge shared by framed controls and the prompt.
const FRAMED_FILL: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.6);
const FRAMED_EDGE: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.15);
/// Paper's floating-panel radius; no radius token matches it.
const PANEL_RADIUS: f32 = 16.0;
/// Playback-information panel fill: #191A21 at 80% over the video.
const PANEL_FILL: Color = Color::from_rgba8(0x19, 0x1a, 0x21, 0.8);
const RAIL: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.22);
const BUFFERED: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.35);
const RAIL_WIDTH: f32 = 4.0;
const KNOB_RADIUS: f32 = 7.0;
const KNOB_BORDER: f32 = 2.5;
const PROGRESS_START: Color = Color::from_rgb8(0x63, 0x66, 0xf1);
const PROGRESS_END: Color = Color::from_rgb8(0x81, 0x8c, 0xf8);
const VOLUME_KNOB_RADIUS: f32 = 4.0;

/// The actual play method, using the information panel's compact status pill.
pub fn play_method_badge<'a, Message: 'a>(label: String, font: iced::Font) -> Element<'a, Message> {
    const GREEN: Color = Color::from_rgb8(0x10, 0xb9, 0x81);
    let dot = container(space())
        .width(6)
        .height(6)
        .style(|_| container::Style {
            background: Some(GREEN.into()),
            border: Border::default().rounded(TOKENS.radii.full),
            shadow: Shadow {
                color: GREEN.scale_alpha(0.6),
                offset: Vector::new(0.0, 0.0),
                blur_radius: 6.0,
            },
            ..container::Style::default()
        });
    let label = text(label)
        .size(10)
        .line_height(iced::Pixels(12.0))
        .font(iced::Font {
            weight: iced::font::Weight::Bold,
            ..font
        });
    container(row![dot, label].spacing(5).align_y(iced::Alignment::Center))
        .height(18)
        .padding([0, 8])
        .align_y(iced::Alignment::Center)
        .style(|_| container::Style {
            background: Some(GREEN.scale_alpha(0.12).into()),
            text_color: Some(GREEN),
            border: Border {
                color: GREEN.scale_alpha(0.28),
                width: 1.0,
                ..Border::default().rounded(TOKENS.radii.full)
            },
            ..container::Style::default()
        })
        .into()
}

/// Reuses the app's Charcoal theme for cinema chrome in either browser theme.
pub fn theme() -> Theme {
    static THEME: std::sync::LazyLock<Theme> =
        std::sync::LazyLock::new(|| crate::theme::theme(crate::theme::ThemeMode::Dark));
    THEME.clone()
}

/// Feedback bubbles and track menus retain their opaque cinema surface.
pub fn popover(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(DARK_PALETTE.colors.surfaceContainer)),
        text_color: Some(DARK_PALETTE.text.body),
        border: Border {
            radius: TOKENS.radii.xl.into(),
            smoothing: super::container::SURFACE_SMOOTHING,
            color: DARK_PALETTE.colors.outlineVariant,
            width: 1.0,
        },
        shadow: DARK_PALETTE.shadows.raised_high.iced(),
        ..container::Style::default()
    }
}

/// Playback-information panel: the Paper floating surface over the video.
pub fn information_panel(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(PANEL_FILL)),
        text_color: Some(DARK_PALETTE.text.body),
        border: Border {
            radius: PANEL_RADIUS.into(),
            smoothing: super::container::SURFACE_SMOOTHING,
            color: DARK_PALETTE.colors.borderSubtle,
            width: 1.0,
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.5),
            offset: Vector::new(0.0, 12.0),
            blur_radius: 32.0,
        },
        ..container::Style::default()
    }
}

/// Episode-queue popover: the opaque container surface at the panel radius.
pub fn queue_panel(theme: &Theme) -> container::Style {
    let mut style = popover(theme);
    style.border.radius = PANEL_RADIUS.into();
    style
}

/// Only painted on queue edges that still conceal scrollable content.
pub fn queue_fade(top: bool) -> container::Style {
    let color = DARK_PALETTE.colors.surfaceContainer;
    let (start, end) = if top {
        (color, Color::TRANSPARENT)
    } else {
        (Color::TRANSPARENT, color)
    };
    container::Style {
        background: Some(Background::Gradient(
            gradient::Linear::new(Degrees(180.0))
                .add_stop(0.0, start)
                .add_stop(1.0, end)
                .into(),
        )),
        ..container::Style::default()
    }
}

/// Compact skip prompt pill floating above the controls.
pub fn prompt(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(FRAMED_FILL)),
        text_color: Some(DARK_PALETTE.text.heading),
        border: Border {
            radius: TOKENS.radii.xl.into(),
            smoothing: super::container::SURFACE_SMOOTHING,
            color: FRAMED_EDGE,
            width: 1.0,
        },
        ..container::Style::default()
    }
}

/// Opaque black navigation controls over the top scrim.
pub fn top_control(theme: &Theme, variant: ButtonVariant, status: button::Status) -> button::Style {
    let mut style = super::button::style(theme, variant, status);
    if variant != ButtonVariant::Primary && status != button::Status::Disabled {
        style.text_color = Color::WHITE;
        style.border.color = FRAMED_EDGE;
        style.border.width = 1.0;
        style.background = Some(Background::Color(match status {
            button::Status::Active if variant == ButtonVariant::TonalActive => {
                DARK_PALETTE.colors.controlHover
            }
            button::Status::Active => Color::BLACK,
            button::Status::Hovered => DARK_PALETTE.colors.controlHover,
            button::Status::Pressed => DARK_PALETTE.colors.surfaceContainer,
            button::Status::Disabled => Color::BLACK,
        }));
    }
    style
}

/// Quiet white transport actions; the primary play/pause action stays filled.
pub fn control(theme: &Theme, variant: ButtonVariant, status: button::Status) -> button::Style {
    let mut style = super::button::style(theme, variant, status);
    if variant == ButtonVariant::Primary {
        style.border.radius = PANEL_RADIUS.into();
        style.background = Some(Background::Color(match status {
            button::Status::Active => DARK_PALETTE.colors.primary.scale_alpha(0.8),
            button::Status::Hovered => DARK_PALETTE.colors.primaryHover.scale_alpha(0.8),
            button::Status::Pressed => DARK_PALETTE.colors.primaryPressed.scale_alpha(0.8),
            button::Status::Disabled => DARK_PALETTE.colors.control,
        }));
    } else {
        style.border.width = 0.0;
        style.border.color = Color::TRANSPARENT;
        style.text_color = if status == button::Status::Disabled {
            DARK_PALETTE.text.muted
        } else {
            Color::WHITE
        };
        if matches!(status, button::Status::Active | button::Status::Disabled)
            && variant != ButtonVariant::TonalActive
        {
            style.background = None;
        }
    }
    style
}

/// Volume feedback changes only the glyph tint, never its hit-area background.
pub fn volume_control(
    theme: &Theme,
    variant: ButtonVariant,
    status: button::Status,
) -> button::Style {
    let mut style = control(theme, variant, status);
    style.background = None;
    style.text_color = match status {
        button::Status::Hovered | button::Status::Pressed => Color::WHITE,
        button::Status::Disabled => DARK_PALETTE.text.muted,
        button::Status::Active => Color::WHITE.scale_alpha(0.85),
    };
    style
}

/// Framed dark output controls, independent of quiet transport.
pub fn framed_control(
    theme: &Theme,
    variant: ButtonVariant,
    status: button::Status,
) -> button::Style {
    let mut style = super::button::style(theme, variant, status);
    if variant != ButtonVariant::Primary && status != button::Status::Disabled {
        style.text_color = Color::WHITE;
        style.border.color = FRAMED_EDGE;
        style.border.width = 1.0;
        style.background = Some(Background::Color(match status {
            button::Status::Active if variant == ButtonVariant::TonalActive => {
                DARK_PALETTE.colors.controlHover
            }
            button::Status::Active => FRAMED_FILL,
            button::Status::Hovered => DARK_PALETTE.colors.controlHover,
            button::Status::Pressed => DARK_PALETTE.colors.surfaceContainer,
            button::Status::Disabled => FRAMED_FILL,
        }));
    }
    style
}

/// Episode-queue row: the playing episode keeps its container highlight even
/// while disabled; other rows stay quiet until hovered.
pub fn queue_row(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let colors = DARK_PALETTE.colors;
        let (background, text_color) = if selected {
            (Some(colors.primaryContainer), colors.secondary)
        } else {
            match status {
                button::Status::Hovered => (Some(colors.controlHover), colors.onControlHover),
                button::Status::Pressed => {
                    (Some(colors.surfaceContainerHigh), DARK_PALETTE.text.heading)
                }
                button::Status::Active | button::Status::Disabled => {
                    (None, DARK_PALETTE.text.secondary)
                }
            }
        };
        button::Style {
            background: background.map(Background::Color),
            text_color,
            border: Border {
                radius: TOKENS.radii.lg.into(),
                smoothing: super::container::SURFACE_SMOOTHING,
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..button::Style::default()
        }
    }
}

/// Fully transparent slider skin; [`timeline_track`] paints the visuals while
/// the slider keeps the pointer and keyboard contract.
pub fn timeline_input(theme: &Theme, status: slider::Status) -> slider::Style {
    let mut style = slider::default(theme, status);
    style.rail.backgrounds = (Color::TRANSPARENT.into(), Color::TRANSPARENT.into());
    style.handle.background = Color::TRANSPARENT.into();
    style.handle.border_color = Color::TRANSPARENT;
    style.handle.border_width = 0.0;
    style
}

/// White volume rail with a small knob revealed only on hover or drag.
pub fn volume(theme: &Theme, status: slider::Status) -> slider::Style {
    let mut style = slider::default(theme, status);
    style.rail.width = RAIL_WIDTH;
    style.rail.backgrounds = (Color::WHITE.into(), RAIL.into());
    style.handle.shape = slider::HandleShape::Circle {
        radius: if status == slider::Status::Active {
            0.0
        } else {
            VOLUME_KNOB_RADIUS
        },
    };
    style.handle.background = Color::WHITE.into();
    style
}

/// Quiet divider between volume and fullscreen controls.
pub fn separator(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::WHITE.scale_alpha(0.15).into()),
        ..container::Style::default()
    }
}

/// Video-edge legibility gradients; the view owns their height and visibility.
pub fn scrim(top: bool) -> container::Style {
    let gradient = if top {
        gradient::Linear::new(Degrees(180.0))
            .add_stop(0.0, Color::BLACK.scale_alpha(0.55))
            .add_stop(0.55, Color::BLACK.scale_alpha(0.25))
            .add_stop(1.0, Color::TRANSPARENT)
    } else {
        gradient::Linear::new(Degrees(180.0))
            .add_stop(0.0, Color::TRANSPARENT)
            .add_stop(0.3, Color::BLACK.scale_alpha(0.22))
            .add_stop(0.55, Color::BLACK.scale_alpha(0.5))
            .add_stop(0.8, Color::BLACK.scale_alpha(0.78))
            .add_stop(1.0, Color::BLACK.scale_alpha(0.92))
    };
    container::Style {
        background: Some(Background::Gradient(gradient.into())),
        ..container::Style::default()
    }
}

/// Non-interactive slider appearance while a playback command is settling.
pub fn unavailable_slider(theme: &Theme, _status: slider::Status) -> slider::Style {
    let mut style = slider::default(theme, slider::Status::Active);
    style.rail.backgrounds = (
        DARK_PALETTE.colors.control.into(),
        DARK_PALETTE.colors.control.into(),
    );
    style.handle.background = DARK_PALETTE.text.muted.into();
    style
}

/// Buffered ranges and gradient progress painted under the transparent seek
/// slider; the slider keeps the pointer and keyboard contract.
pub fn timeline_track<'a, Message: 'a>(
    position: f64,
    duration: f64,
    buffered: &'a [(f64, f64)],
    available: bool,
) -> Element<'a, Message> {
    Element::new(TimelineTrack {
        position,
        duration,
        buffered,
        available,
    })
}

struct TimelineTrack<'a> {
    position: f64,
    duration: f64,
    buffered: &'a [(f64, f64)],
    available: bool,
}

impl<Message> Widget<Message, Theme, iced::Renderer> for TimelineTrack<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut widget::Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, Length::Fill, Length::Fill)
    }

    fn draw(
        &self,
        _tree: &widget::Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: iced::mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        if bounds.width <= 0.0 || !self.duration.is_finite() || self.duration <= 0.0 {
            return;
        }
        let rail = Rectangle {
            x: bounds.x,
            y: bounds.y + (bounds.height - RAIL_WIDTH) / 2.0,
            width: bounds.width,
            height: RAIL_WIDTH,
        };
        let rail_border = Border {
            radius: (RAIL_WIDTH / 2.0).into(),
            ..Border::default()
        };
        renderer.with_layer(*viewport, |renderer| {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: rail,
                    border: rail_border,
                    ..renderer::Quad::default()
                },
                RAIL,
            );
            for &(start, end) in self.buffered {
                let left = (start / self.duration).clamp(0.0, 1.0) as f32;
                let right = (end / self.duration).clamp(0.0, 1.0) as f32;
                if right <= left {
                    continue;
                }
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle {
                            x: rail.x + left * rail.width,
                            width: (right - left) * rail.width,
                            ..rail
                        },
                        border: rail_border,
                        ..renderer::Quad::default()
                    },
                    BUFFERED,
                );
            }
            let fraction = (self.position / self.duration).clamp(0.0, 1.0) as f32;
            let progress = Rectangle {
                width: fraction * rail.width,
                ..rail
            };
            if progress.width > 0.0 {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: progress,
                        border: rail_border,
                        shadow: if self.available {
                            Shadow {
                                color: DARK_PALETTE.colors.primary.scale_alpha(0.5),
                                offset: Vector::new(0.0, 0.0),
                                blur_radius: 8.0,
                            }
                        } else {
                            Shadow::default()
                        },
                        ..renderer::Quad::default()
                    },
                    if self.available {
                        Background::Gradient(
                            gradient::Linear::new(Degrees(90.0))
                                .add_stop(0.0, PROGRESS_START)
                                .add_stop(1.0, PROGRESS_END)
                                .into(),
                        )
                    } else {
                        DARK_PALETTE.colors.control.into()
                    },
                );
            }
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: bounds.x + fraction * bounds.width - KNOB_RADIUS,
                        y: bounds.y + bounds.height / 2.0 - KNOB_RADIUS,
                        width: KNOB_RADIUS * 2.0,
                        height: KNOB_RADIUS * 2.0,
                    },
                    border: Border {
                        color: Color::WHITE,
                        width: KNOB_BORDER,
                        radius: KNOB_RADIUS.into(),
                        ..Border::default()
                    },
                    shadow: Shadow {
                        color: Color::BLACK.scale_alpha(0.5),
                        offset: Vector::new(0.0, 2.0),
                        blur_radius: 8.0,
                    },
                    ..renderer::Quad::default()
                },
                if self.available {
                    DARK_PALETTE.colors.secondary
                } else {
                    DARK_PALETTE.text.muted
                },
            );
        });
    }
}
