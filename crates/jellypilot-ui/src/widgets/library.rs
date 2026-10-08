//! Shared library and personal-list catalog treatments.

use iced::widget::{button, container, progress_bar, rule};
use iced::{Border, Color, Theme};

use crate::tokens::{palette, LANDSCAPE_PROGRESS_TRACK, POSTER_PROGRESS_TRACK, TOKENS};
use crate::variants::ButtonVariant;

/// Circular filled play action over media artwork; the caller supplies its
/// 48px target and ControlButton retains the visible keyboard focus ring.
pub fn card_play(theme: &Theme, variant: ButtonVariant, status: button::Status) -> button::Style {
    let mut style = super::button::style(theme, variant, status);
    style.border.radius = TOKENS.radii.full.into();
    // Continuous corner smoothing would turn the circular target into a superellipse.
    style.border.smoothing = 0.0;
    style
}

pub fn segment(theme: &Theme, status: button::Status, selected: bool) -> button::Style {
    let colors = palette(theme).colors;
    let background = if selected {
        colors.primaryContainer
    } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
        colors.controlHover
    } else {
        Color::TRANSPARENT
    };
    button::Style {
        background: Some(background.into()),
        text_color: if selected {
            colors.secondary
        } else {
            palette(theme).text.metadata
        },
        border: Border::default().rounded(TOKENS.radii.lg),
        ..button::Style::default()
    }
}

pub fn segments(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(palette(theme).colors.surfaceContainerHigh.into()),
        border: Border::default().rounded(TOKENS.radii.lg),
        ..container::Style::default()
    }
}

pub fn row(theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: matches!(status, button::Status::Hovered | button::Status::Pressed)
            .then(|| palette(theme).colors.controlHover.into()),
        text_color: palette(theme).text.body,
        border: Border::default().rounded(TOKENS.radii.xl),
        ..button::Style::default()
    }
}

pub fn divider(theme: &Theme) -> rule::Style {
    rule::Style {
        color: palette(theme).colors.borderSubtle,
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    }
}

pub fn progress(theme: &Theme) -> progress_bar::Style {
    progress_bar::Style {
        background: palette(theme).colors.surfaceContainerHigh.into(),
        bar: palette(theme).colors.primary.into(),
        border: Border::default().rounded(TOKENS.radii.full),
    }
}

pub fn poster_placeholder(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(palette(theme).colors.surfaceContainerHigh.into()),
        border: Border::default()
            .rounded(TOKENS.radii.xl)
            .smoothing(super::container::SURFACE_SMOOTHING),
        ..container::Style::default()
    }
}

pub fn artwork_progress_style(theme: &Theme, poster: bool) -> super::artwork_progress::Style {
    super::artwork_progress::Style {
        fill: palette(theme).colors.primary,
        track: if poster {
            POSTER_PROGRESS_TRACK
        } else {
            LANDSCAPE_PROGRESS_TRACK
        },
        blur: if poster { 10.0 } else { 0.0 },
    }
}

pub fn artwork_outline(theme: &Theme, compact: bool) -> container::Style {
    container::Style {
        border: Border {
            color: palette(theme).colors.imageOutline,
            width: 1.0,
            radius: if compact {
                TOKENS.radii.lg
            } else {
                TOKENS.radii.xl
            }
            .into(),
            ..Border::default()
        }
        .smoothing(super::container::SURFACE_SMOOTHING),
        ..container::Style::default()
    }
}

pub fn rating(_theme: &Theme) -> container::Style {
    let palette = &crate::tokens::DARK_PALETTE;
    container::Style {
        background: Some(palette.colors.background.scale_alpha(0.65).into()),
        text_color: Some(palette.text.secondary),
        border: Border::default().rounded(TOKENS.radii.lg),
        ..container::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{theme, ThemeMode};
    use iced::advanced::graphics::shape::RoundedRectangle;
    use iced::{Point, Rectangle, Size};

    #[test]
    fn artwork_play_action_has_a_circular_contour_in_every_state() {
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(48.0, 48.0));
        let radius = bounds.width / 2.0;
        for mode in [ThemeMode::Dark, ThemeMode::Light] {
            let theme = theme(mode);
            for status in [
                button::Status::Active,
                button::Status::Hovered,
                button::Status::Pressed,
                button::Status::Disabled,
            ] {
                let border = card_play(&theme, ButtonVariant::Primary, status).border;
                let contour = RoundedRectangle::new(bounds, border.radius, border.smoothing)
                    .expect("valid play action contour");
                for angle in [
                    0.0,
                    std::f32::consts::FRAC_PI_4,
                    std::f32::consts::FRAC_PI_2,
                ] {
                    let point = bounds.center()
                        + iced::Vector::new(radius * angle.cos(), radius * angle.sin());
                    assert!(
                        contour.distance(point).abs() < 0.001,
                        "a circular action must have the same radius in every direction: {status:?}, angle={angle}"
                    );
                }
            }
        }
    }
}
