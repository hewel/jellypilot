//! JellyPilot iced theme and widget catalog entry points.

use iced::theme::Palette;
use iced::widget::{button, container, scrollable as iced_scrollable, text_input};
use iced::{Color, Theme};

use crate::tokens::TOKENS;
use crate::variants::{BadgeVariant, ButtonVariant, FieldVariant, SurfaceVariant, TextVariant};
use crate::widgets;

/// Builds the fixed dark JellyPilot iced theme.
pub fn theme() -> Theme {
    Theme::custom(
        "JellyPilot Dark",
        Palette {
            background: TOKENS.raw_colors.neutral.n975,
            text: TOKENS.raw_colors.neutral.n50,
            primary: TOKENS.raw_colors.indigo.n600,
            success: TOKENS.raw_colors.teal.n400,
            warning: TOKENS.raw_colors.amber.n400,
            danger: TOKENS.raw_colors.red.n400,
        },
    )
}

/// Resolves a semantic text role.
pub fn text_variant(_theme: &Theme, variant: TextVariant) -> Color {
    match variant {
        TextVariant::OnSurface => TOKENS.colors.onSurface,
        TextVariant::OnSurfaceVariant => TOKENS.colors.onSurfaceVariant,
        TextVariant::Primary => TOKENS.colors.primary,
        TextVariant::Secondary => TOKENS.colors.secondary,
        TextVariant::Tertiary => TOKENS.colors.tertiary,
        TextVariant::Warning => TOKENS.colors.warning,
        TextVariant::Error => TOKENS.colors.error,
    }
}

/// Resolves a card surface variant.
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
