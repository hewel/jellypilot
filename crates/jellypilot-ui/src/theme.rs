//! JellyPilot iced theme and widget catalog entry points.

use iced::theme::Palette;
use iced::widget::{button, container, scrollable as iced_scrollable, text_input};
use iced::{Color, Theme};

use crate::tokens::{palette, DARK_PALETTE, LIGHT_PALETTE};
use crate::variants::{BadgeVariant, ButtonVariant, FieldVariant, SurfaceVariant, TextVariant};
use crate::widgets;

/// Theme mode the app can pin explicitly. `System` is an app-level concern:
/// the app resolves it against the OS mode before calling [`theme`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Dark,
    Light,
}

/// Builds the JellyPilot iced theme for the given mode. The theme's
/// `Palette.background` doubles as the palette identity that
/// [`crate::tokens::palette`] resolves against.
pub fn theme(mode: ThemeMode) -> Theme {
    let colors = match mode {
        ThemeMode::Dark => DARK_PALETTE.colors,
        ThemeMode::Light => LIGHT_PALETTE.colors,
    };
    Theme::custom(
        match mode {
            ThemeMode::Dark => "JellyPilot Dark",
            ThemeMode::Light => "JellyPilot Light",
        },
        Palette {
            background: colors.background,
            text: colors.onSurface,
            primary: colors.primary,
            success: colors.tertiary,
            warning: colors.warning,
            danger: colors.error,
        },
    )
}

/// Resolves a semantic text role.
pub fn text_variant(theme: &Theme, variant: TextVariant) -> Color {
    let colors = palette(theme).colors;
    match variant {
        TextVariant::OnSurface => colors.onSurface,
        TextVariant::OnSurfaceVariant => colors.onSurfaceVariant,
        TextVariant::Primary => colors.primary,
        TextVariant::Secondary => colors.secondary,
        TextVariant::Tertiary => colors.tertiary,
        TextVariant::Warning => colors.warning,
        TextVariant::Error => colors.error,
    }
}

/// Resolves a container surface role.
pub fn surface_variant(theme: &Theme, variant: SurfaceVariant) -> container::Style {
    widgets::container::style(theme, variant)
}

/// Resolves a button variant and interaction status.
pub fn button_variant(
    theme: &Theme,
    status: button::Status,
    variant: ButtonVariant,
) -> button::Style {
    widgets::button::style(theme, variant, status)
}

/// Resolves a status badge variant.
pub fn badge_variant(theme: &Theme, variant: BadgeVariant) -> container::Style {
    widgets::badge::style(theme, variant)
}

/// Resolves a normal text-input variant and interaction status.
pub fn field_variant(
    theme: &Theme,
    status: text_input::Status,
    variant: FieldVariant,
) -> text_input::Style {
    widgets::field::style(theme, variant, status)
}

/// Resolves an invalid text-input variant and interaction status.
pub fn error_field_variant(
    theme: &Theme,
    status: text_input::Status,
    variant: FieldVariant,
) -> text_input::Style {
    widgets::field::error_style(theme, variant, status)
}

/// Resolves JellyPilot scrollbar chrome for the current interaction status.
pub fn scrollable(theme: &Theme, status: iced_scrollable::Status) -> iced_scrollable::Style {
    widgets::scrollable::style(theme, status)
}
