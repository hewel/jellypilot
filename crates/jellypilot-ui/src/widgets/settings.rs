//! Settings-only surfaces and selection controls from the Paper reference.

use iced::widget::{button, container};
use iced::{Border, Theme};

use crate::tokens::{palette, SETTINGS_MODAL_RADIUS, TOKENS};
use crate::variants::{ButtonVariant, SurfaceVariant};

/// Flat content surface shared by the wide panel and compact full-window view.
pub fn content(theme: &Theme) -> container::Style {
    let colors = palette(theme).colors;
    container::Style {
        background: Some(colors.surface.into()),
        text_color: Some(colors.onSurface),
        ..container::Style::default()
    }
}

/// Full-height section navigation, distinct from the application's Sidebar.
pub fn navigation(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(palette(theme).colors.surfaceVariant.into()),
        border: Border {
            radius: iced::border::Radius {
                top_left: SETTINGS_MODAL_RADIUS - 1.0,
                bottom_left: SETTINGS_MODAL_RADIUS - 1.0,
                ..iced::border::Radius::default()
            },
            smoothing: super::container::SURFACE_SMOOTHING,
            ..Border::default()
        },
        ..content(theme)
    }
}

/// Floating Settings shell; other dialogs keep their existing geometry.
pub fn dialog(theme: &Theme) -> container::Style {
    let mut style = super::container::style(theme, SurfaceVariant::Dialog);
    style.background = Some(palette(theme).colors.surface.into());
    style.border.radius = SETTINGS_MODAL_RADIUS.into();
    style.border.color = palette(theme).colors.borderSubtle;
    style
}

/// Section selection uses the Paper primary-container and secondary-content pair.
pub fn navigation_button(
    theme: &Theme,
    variant: ButtonVariant,
    status: button::Status,
) -> button::Style {
    let mut style = super::button::style(theme, variant, status);
    if variant == ButtonVariant::Secondary && status != button::Status::Disabled {
        let colors = palette(theme).colors;
        style.background = Some(colors.primaryContainer.into());
        style.text_color = colors.secondary;
    }
    style
}

/// Inset options inside a single segmented-control surface.
pub fn segmented_button(
    theme: &Theme,
    variant: ButtonVariant,
    status: button::Status,
) -> button::Style {
    let mut style = navigation_button(theme, variant, status);
    style.border.radius = TOKENS.radii.lg.into();
    style
}

/// Quiet shared track around mutually exclusive appearance choices.
pub fn segmented_group(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(palette(theme).colors.surfaceContainerHigh.into()),
        border: Border {
            radius: TOKENS.radii.lg.into(),
            smoothing: super::container::SURFACE_SMOOTHING,
            ..Border::default()
        },
        ..container::Style::default()
    }
}

/// Current account identity's quiet inset surface.
pub fn account_identity(theme: &Theme) -> container::Style {
    let mut style = account_address(theme);
    style.background = Some(palette(theme).colors.surfaceContainerLow.into());
    style
}

/// Server address inset, without card borders or elevation.
pub fn account_address(theme: &Theme) -> container::Style {
    let mut style = segmented_group(theme);
    style.border.radius = TOKENS.radii.xl.into();
    style
}

/// Keeps real account photos distinct from the panel in either theme.
pub fn avatar_outline(theme: &Theme, radius: f32) -> container::Style {
    container::Style {
        border: Border {
            radius: radius.into(),
            smoothing: super::container::SURFACE_SMOOTHING,
            color: palette(theme).colors.imageOutline,
            width: 1.0,
        },
        ..container::Style::default()
    }
}

/// Keyboard hint inset in the section navigation footer.
pub fn keycap(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(palette(theme).colors.surfaceContainerHigh.into()),
        border: Border {
            radius: TOKENS.radii.md.into(),
            color: palette(theme).colors.borderSubtle,
            width: 1.0,
            ..Border::default()
        },
        ..container::Style::default()
    }
}

/// Neutral separator for the fixed compact header and About group.
pub fn divider(theme: &Theme) -> container::Style {
    container::Style::default().background(palette(theme).colors.borderSubtle)
}

/// Flat diagnostic event row; shares the quiet account inset treatment.
pub fn event_card(theme: &Theme) -> container::Style {
    account_identity(theme)
}
