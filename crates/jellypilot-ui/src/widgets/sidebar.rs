//! Opt-in Sidebar treatments; shared widget defaults remain unchanged.

use iced::widget::{button, container, text_input};
use iced::{Background, Border, Color, Theme};

use crate::tokens::{
    palette, ACCOUNT_POPOVER_RADIUS, LIGHT_PALETTE, SIDEBAR_CONTROL_RADIUS, SIDEBAR_INSET_RADIUS,
    TOKENS,
};
use crate::variants::{ButtonVariant, FieldVariant};
use crate::widgets::container::SURFACE_SMOOTHING;

fn surface(theme: &Theme, fill: Color, radius: f32, outlined: bool) -> container::Style {
    container::Style {
        background: Some(Background::Color(fill)),
        text_color: Some(palette(theme).text.body),
        border: Border {
            smoothing: if radius == 0.0 {
                0.0
            } else {
                SURFACE_SMOOTHING
            },
            radius: radius.into(),
            color: palette(theme).colors.outlineVariant,
            width: if outlined { 1.0 } else { 0.0 },
        },
        ..container::Style::default()
    }
}

/// Search content blends into the pill frame: never draw its own border or
/// background, because the surrounding [`SearchField`] owns both.
pub fn search_input(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let mut style = super::field::style(theme, FieldVariant::Filled, status);
    style.background = Background::Color(Color::TRANSPARENT);
    style.border = Border {
        smoothing: SURFACE_SMOOTHING,
        radius: TOKENS.radii.lg.into(),
        color: Color::TRANSPARENT,
        width: 0.0,
    };
    style
}

/// Neutral structural separator for the docked tools and account actions.
pub fn divider(theme: &Theme) -> container::Style {
    container::Style::default().background(palette(theme).colors.outlineVariant)
}

/// Inset server address surface in the account quick menu.
pub fn address(theme: &Theme) -> container::Style {
    surface(
        theme,
        palette(theme).colors.surfaceContainerHigh,
        TOKENS.radii.lg,
        false,
    )
}

/// Account-only floating surface; other popovers retain their default appearance.
pub fn popover(theme: &Theme) -> container::Style {
    let colors = palette(theme).colors;
    let fill = if colors.background == LIGHT_PALETTE.colors.background {
        colors.surface
    } else {
        colors.surfaceContainer
    };
    let mut style = surface(theme, fill, ACCOUNT_POPOVER_RADIUS, true);
    style.shadow = palette(theme).shadows.raised_high.iced();
    style
}

/// Rounded personal destination, preserving the existing variant's interaction colors.
pub fn personal(theme: &Theme, variant: ButtonVariant, status: button::Status) -> button::Style {
    let mut style = super::button::style(theme, variant, status);
    style.border.radius = SIDEBAR_CONTROL_RADIUS.into();
    style
}

/// Library destinations share the personal-navigation geometry.
pub fn library(theme: &Theme, variant: ButtonVariant, status: button::Status) -> button::Style {
    personal(theme, variant, status)
}

/// Neutral account anchor; opening the menu does not alter the structural outline.
pub fn identity(theme: &Theme, variant: ButtonVariant, status: button::Status) -> button::Style {
    let mut style = personal(theme, variant, status);
    let colors = palette(theme).colors;
    style.background = Some(Background::Color(match status {
        button::Status::Hovered | button::Status::Pressed => colors.control,
        _ if colors.background == LIGHT_PALETTE.colors.background => colors.surface,
        _ => colors.surfaceContainerLow,
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

/// Search-pill inset actions: [`action`] geometry at the concentric inset
/// radius inside the 12 px pill frame.
pub fn search_action(
    theme: &Theme,
    variant: ButtonVariant,
    status: button::Status,
) -> button::Style {
    let mut style = action(theme, variant, status);
    style.border.radius = SIDEBAR_INSET_RADIUS.into();
    style
}

/// Quiet account-menu rows: hover and press provide the surface, not resting actions.
pub fn menu_action(theme: &Theme, variant: ButtonVariant, status: button::Status) -> button::Style {
    let mut style = action(theme, variant, status);
    if matches!(status, button::Status::Active | button::Status::Disabled) {
        style.background = None;
        style.border.width = 0.0;
        style.border.color = Color::TRANSPARENT;
    }
    style
}

#[cfg(test)]
mod tests {
    use iced::widget::text_input::Status;
    use iced::{Background, Color};

    use super::{search_input, ButtonVariant};
    use crate::variants::FieldVariant;

    fn field_style(theme: &iced::Theme, status: Status) -> iced::widget::text_input::Style {
        crate::widgets::field::style(theme, FieldVariant::Filled, status)
    }

    #[test]
    fn search_input_never_draws_its_own_border_or_background() {
        let theme = crate::theme::theme(crate::theme::ThemeMode::Dark);
        for status in [
            Status::Active,
            Status::Hovered,
            Status::Focused { is_hovered: false },
            Status::Focused { is_hovered: true },
            Status::Disabled,
        ] {
            let style = search_input(&theme, status);
            assert_eq!(
                style.background,
                Background::Color(Color::TRANSPARENT),
                "search_input background must be transparent in {status:?}"
            );
            assert_eq!(
                style.border.width, 0.0,
                "search_input border must be hidden in {status:?}"
            );
            assert_eq!(
                style.border.color,
                Color::TRANSPARENT,
                "search_input border must be transparent in {status:?}"
            );
        }
    }

    #[test]
    fn search_input_preserves_field_content_colors() {
        let theme = crate::theme::theme(crate::theme::ThemeMode::Dark);
        for status in [
            Status::Active,
            Status::Hovered,
            Status::Focused { is_hovered: false },
            Status::Disabled,
        ] {
            let got = search_input(&theme, status);
            let expected = field_style(&theme, status);
            assert_eq!(
                got.placeholder, expected.placeholder,
                "placeholder color in {status:?}"
            );
            assert_eq!(got.value, expected.value, "value color in {status:?}");
            assert_eq!(
                got.selection, expected.selection,
                "selection color in {status:?}"
            );
        }
    }

    #[test]
    fn menu_action_is_borderless_and_transparent_at_rest_and_disabled() {
        use iced::widget::button;

        let theme = crate::theme::theme(crate::theme::ThemeMode::Dark);
        for status in [button::Status::Active, button::Status::Disabled] {
            let style = super::menu_action(&theme, ButtonVariant::Tonal, status);
            assert_eq!(
                style.background, None,
                "menu_action must have no background in {status:?}"
            );
            assert_eq!(
                style.border.width, 0.0,
                "menu_action must have no structural border in {status:?}"
            );
            assert_eq!(
                style.border.color,
                Color::TRANSPARENT,
                "menu_action border must be transparent in {status:?}"
            );
        }
    }

    #[test]
    fn menu_action_keeps_hover_and_pressed_surface() {
        use iced::widget::button;

        let theme = crate::theme::theme(crate::theme::ThemeMode::Dark);
        for status in [button::Status::Hovered, button::Status::Pressed] {
            let style = super::menu_action(&theme, ButtonVariant::Tonal, status);
            assert!(
                style.background.is_some(),
                "menu_action must keep its surface in {status:?}"
            );
        }
    }
}
