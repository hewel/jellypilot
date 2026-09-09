//! Shared library and personal-list catalog treatments.

use iced::widget::{button, container, progress_bar, rule};
use iced::{Border, Color, Theme};

use crate::tokens::{palette, LANDSCAPE_PROGRESS_TRACK, POSTER_PROGRESS_TRACK, TOKENS};

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
        border: Border::default().rounded(TOKENS.radii.xl),
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
