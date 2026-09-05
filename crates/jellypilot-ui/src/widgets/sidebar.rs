//! Opt-in Sidebar treatments; shared widget defaults remain unchanged.

use iced::widget::{button, container, text_input};
use iced::{Background, Border, Color, Theme};

use crate::tokens::{palette, ACCOUNT_POPOVER_RADIUS, SIDEBAR_CONTROL_RADIUS, TOKENS};
use crate::variants::{ButtonVariant, FieldVariant};

fn surface(theme: &Theme, fill: Color, radius: f32, outlined: bool) -> container::Style {
    container::Style {
        background: Some(Background::Color(fill)),
        text_color: Some(palette(theme).text.body),
        border: Border {
            radius: radius.into(),
            color: palette(theme).colors.outlineVariant,
            width: if outlined { 1.0 } else { 0.0 },
        },
        ..container::Style::default()
    }
}

/// Quiet search surface shared by expanded and compact Sidebar search.
pub fn search(theme: &Theme) -> container::Style {
    surface(
        theme,
        palette(theme).colors.control,
        SIDEBAR_CONTROL_RADIUS,
        true,
    )
}

/// Search content blends into its outer surface while preserving field focus feedback.
pub fn search_input(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let mut style = super::field::style(theme, FieldVariant::Filled, status);
    style.background = Background::Color(palette(theme).colors.control);
    style.border.radius = TOKENS.radii.lg.into();
    style
}

/// Docked toolbar with a neutral fill and no floating elevation.
pub fn toolbar(theme: &Theme) -> container::Style {
    surface(
        theme,
        palette(theme).colors.control,
        SIDEBAR_CONTROL_RADIUS,
        false,
    )
}

/// Neutral structural separator between toolbar actions.
pub fn divider(theme: &Theme) -> container::Style {
    container::Style::default().background(palette(theme).colors.outlineVariant)
}

/// Bounded inset control surface for compact Sidebar details.
pub fn inset(theme: &Theme) -> container::Style {
    surface(theme, palette(theme).colors.control, TOKENS.radii.lg, true)
}

/// Quiet count badge, independent from the heading text.
pub fn count_badge(theme: &Theme) -> container::Style {
    surface(theme, palette(theme).colors.control, TOKENS.radii.md, false)
}

/// Account-only floating surface; other popovers retain their default appearance.
pub fn popover(theme: &Theme) -> container::Style {
    let mut style = surface(
        theme,
        palette(theme).colors.surface,
        ACCOUNT_POPOVER_RADIUS,
        true,
    );
    style.shadow = palette(theme).shadows.raised_high.iced();
    style
}

/// Rounded personal destination, preserving the existing variant's interaction colors.
pub fn personal(theme: &Theme, variant: ButtonVariant, status: button::Status) -> button::Style {
    let mut style = super::button::style(theme, variant, status);
    style.border.radius = SIDEBAR_CONTROL_RADIUS.into();
    style
}

/// Denser library destination with the Sidebar's inset radius.
pub fn library(theme: &Theme, variant: ButtonVariant, status: button::Status) -> button::Style {
    action(theme, variant, status)
}

/// Neutral account anchor; opening the menu does not alter the structural outline.
pub fn identity(theme: &Theme, variant: ButtonVariant, status: button::Status) -> button::Style {
    let mut style = personal(theme, variant, status);
    let colors = palette(theme).colors;
    style.background = Some(Background::Color(match status {
        button::Status::Hovered | button::Status::Pressed => colors.control,
        _ => colors.surface,
    }));
    style.border.color = colors.outlineVariant;
    style.border.width = 1.0;
    style
}

/// Inset action geometry without altering global action variants.
pub fn action(theme: &Theme, variant: ButtonVariant, status: button::Status) -> button::Style {
    let mut style = super::button::style(theme, variant, status);
    style.border.radius = TOKENS.radii.lg.into();
    style
}

/// Quiet account-menu rows: hover and press provide the surface, not resting actions.
pub fn menu_action(theme: &Theme, variant: ButtonVariant, status: button::Status) -> button::Style {
    let mut style = action(theme, variant, status);
    if matches!(status, button::Status::Active | button::Status::Disabled) {
        style.background = None;
    }
    style
}
