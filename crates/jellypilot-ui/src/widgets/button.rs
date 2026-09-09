//! JellyPilot button catalog styles.

use iced::widget::button;
use iced::{Background, Border, Color, Theme};

use crate::tokens::{palette, LIGHT_PALETTE, TOKENS};
use crate::variants::ButtonVariant;

/// Chrome-free artwork hit surface; ControlButton supplies its focus ring.
pub fn media_artwork(
    _theme: &Theme,
    _variant: ButtonVariant,
    _status: button::Status,
) -> button::Style {
    button::Style {
        snap: false,
        border: Border {
            radius: super::rounded_image::full_radius(TOKENS.radii.xl),
            ..Border::default()
        }
        .smoothing(super::container::SURFACE_SMOOTHING),
        ..button::Style::default()
    }
}

/// Resolves a button variant and interaction status to an iced style.
pub fn style(theme: &Theme, variant: ButtonVariant, status: button::Status) -> button::Style {
    let palette = palette(theme);
    let colors = palette.colors;
    let text = palette.text;

    let mut background: Option<Color>;
    let mut text_color: Color;
    let mut border_color = Color::TRANSPARENT;
    let mut border_width = 0.0;

    let radius = match variant {
        ButtonVariant::Pill | ButtonVariant::PillActive => TOKENS.radii.lg,
        _ => TOKENS.radii.xl,
    };

    match variant {
        ButtonVariant::Primary => {
            background = Some(colors.primary);
            text_color = colors.onPrimary;
        }
        ButtonVariant::Secondary => {
            background = Some(colors.secondaryContainer);
            text_color = colors.onSecondaryContainer;
        }
        ButtonVariant::Tonal | ButtonVariant::Icon | ButtonVariant::Pill => {
            background = Some(colors.surfaceContainerHigh);
            text_color = colors.onControl;
            border_color = colors.borderSubtle;
            border_width = 1.0;
        }
        ButtonVariant::TonalActive => {
            background = Some(colors.controlHover);
            text_color = colors.onControlHover;
            border_color = colors.borderSubtle;
            border_width = 1.0;
        }
        ButtonVariant::Text => {
            background = None;
            text_color = text.body;
        }
        ButtonVariant::PillActive => {
            background = Some(colors.primaryContainer);
            text_color = colors.secondary;
        }
    }

    match status {
        button::Status::Active => {}
        button::Status::Hovered => match variant {
            ButtonVariant::Primary => {
                background = Some(colors.primaryHover);
            }
            ButtonVariant::Tonal | ButtonVariant::Icon | ButtonVariant::Pill => {
                background = Some(colors.controlHover);
                text_color = colors.onControlHover;
            }
            ButtonVariant::Text => {
                background = Some(colors.control);
                text_color = text.heading;
            }
            ButtonVariant::Secondary | ButtonVariant::TonalActive | ButtonVariant::PillActive => {}
        },
        button::Status::Pressed => match variant {
            ButtonVariant::Primary => {
                background = Some(colors.primaryPressed);
            }
            ButtonVariant::Tonal | ButtonVariant::Icon | ButtonVariant::Pill => {
                background = Some(colors.surfaceContainer);
                text_color = colors.onControl;
            }
            ButtonVariant::Text => {
                background = None;
                text_color = text.body;
            }
            ButtonVariant::Secondary | ButtonVariant::TonalActive | ButtonVariant::PillActive => {}
        },
        button::Status::Disabled => {
            background = Some(colors.control);
            text_color = text.muted;
            border_width = 0.0;
            border_color = Color::TRANSPARENT;
        }
    }

    button::Style {
        background: background.map(Background::Color),
        text_color,
        border: Border {
            smoothing: super::container::SURFACE_SMOOTHING,
            radius: radius.into(),
            color: border_color,
            width: border_width,
        },
        shadow: palette.shadows.none.iced(),
        ..button::Style::default()
    }
}

/// Translucent Hero fills over the native Backdrop blur.
///
/// * Primary keeps its solid accent fill (`primary` / `primaryHover` /
///   `primaryPressed`).
/// * Non-primary variants use a borderless 10% tint, lifted to 24% on hover
///   and 18% when pressed. Dark mode uses white; light mode uses on-surface
///   ink to stay legible over its light fade. Keyboard focus is separate.
pub fn hero_glass(theme: &Theme, variant: ButtonVariant, status: button::Status) -> button::Style {
    let mut style = style(theme, variant, status);

    if variant == ButtonVariant::Primary {
        return style;
    }

    style.border.width = 0.0;
    style.border.color = Color::TRANSPARENT;

    let fill_alpha = match status {
        button::Status::Active => 0.10,
        button::Status::Hovered => 0.24,
        button::Status::Pressed => 0.18,
        button::Status::Disabled => return style,
    };

    let colors = palette(theme).colors;
    let ink = if colors.background == LIGHT_PALETTE.colors.background {
        colors.onSurface
    } else {
        Color::WHITE
    };
    style.background = Some(Background::Color(ink.scale_alpha(fill_alpha)));
    style.text_color = ink;
    style
}

/// Detail navigation over the hero's sampled, blurred backdrop.
pub fn detail_back(theme: &Theme, variant: ButtonVariant, status: button::Status) -> button::Style {
    let mut style = style(theme, variant, status);
    let palette = palette(theme);
    let alpha = match status {
        button::Status::Hovered => 0.5,
        button::Status::Pressed => 0.6,
        button::Status::Disabled => 0.2,
        button::Status::Active => 0.4,
    };
    style.background = Some(palette.colors.background.scale_alpha(alpha).into());
    style.border.width = 0.0;
    style.border.color = Color::TRANSPARENT;
    style.text_color = palette
        .text
        .heading
        .scale_alpha(if status == button::Status::Disabled {
            0.5
        } else {
            1.0
        });
    style
}

#[cfg(test)]
mod tests {
    use iced::border::Radius;
    use iced::widget::button::Status;
    use iced::{Background, Color, Shadow};

    use crate::tokens::{DARK_PALETTE, TOKENS};
    use crate::variants::ButtonVariant;

    fn dark_theme() -> iced::Theme {
        crate::theme::theme(crate::theme::ThemeMode::Dark)
    }

    #[test]
    fn button_variants_use_correct_radius_token() {
        for variant in [
            ButtonVariant::Primary,
            ButtonVariant::Secondary,
            ButtonVariant::Tonal,
            ButtonVariant::TonalActive,
            ButtonVariant::Text,
            ButtonVariant::Icon,
        ] {
            let style = super::style(&dark_theme(), variant, Status::Active);
            assert_eq!(
                style.border.radius,
                Radius::from(TOKENS.radii.xl),
                "{variant:?} must use the xl (12px) radius token"
            );
        }

        for variant in [ButtonVariant::Pill, ButtonVariant::PillActive] {
            let style = super::style(&dark_theme(), variant, Status::Active);
            assert_eq!(
                style.border.radius,
                Radius::from(TOKENS.radii.lg),
                "{variant:?} must use the lg (8px) radius token"
            );
        }
    }

    #[test]
    fn buttons_cast_no_shadow_in_any_status() {
        let theme = dark_theme();
        for variant in [
            ButtonVariant::Primary,
            ButtonVariant::Secondary,
            ButtonVariant::Tonal,
            ButtonVariant::TonalActive,
            ButtonVariant::Text,
            ButtonVariant::Icon,
            ButtonVariant::Pill,
            ButtonVariant::PillActive,
        ] {
            for status in [
                Status::Active,
                Status::Hovered,
                Status::Pressed,
                Status::Disabled,
            ] {
                let style = super::style(&theme, variant, status);
                assert_eq!(
                    style.shadow,
                    Shadow::default(),
                    "Button variant {variant:?} must cast no shadow in status {status:?}"
                );
            }
        }
    }

    #[test]
    fn primary_uses_accent_hover_and_pressed_tokens() {
        let theme = dark_theme();
        let colors = DARK_PALETTE.colors;

        let active = super::style(&theme, ButtonVariant::Primary, Status::Active);
        assert_eq!(active.background, Some(Background::Color(colors.primary)));
        assert_eq!(active.text_color, colors.onPrimary);
        assert_eq!(active.border.width, 0.0);

        let hovered = super::style(&theme, ButtonVariant::Primary, Status::Hovered);
        assert_eq!(
            hovered.background,
            Some(Background::Color(colors.primaryHover))
        );
        assert_eq!(hovered.text_color, colors.onPrimary);

        let pressed = super::style(&theme, ButtonVariant::Primary, Status::Pressed);
        assert_eq!(
            pressed.background,
            Some(Background::Color(colors.primaryPressed))
        );
        assert_eq!(pressed.text_color, colors.onPrimary);
    }

    #[test]
    fn tonal_uses_surface_container_and_control_hover_tokens() {
        let theme = dark_theme();
        let colors = DARK_PALETTE.colors;

        let active = super::style(&theme, ButtonVariant::Tonal, Status::Active);
        assert_eq!(
            active.background,
            Some(Background::Color(colors.surfaceContainerHigh))
        );
        assert_eq!(active.text_color, colors.onControl);
        assert_eq!(active.border.width, 1.0);
        assert_eq!(active.border.color, colors.borderSubtle);

        let hovered = super::style(&theme, ButtonVariant::Tonal, Status::Hovered);
        assert_eq!(
            hovered.background,
            Some(Background::Color(colors.controlHover))
        );
        assert_eq!(hovered.text_color, colors.onControlHover);
        assert_eq!(hovered.border.color, colors.borderSubtle);

        let pressed = super::style(&theme, ButtonVariant::Tonal, Status::Pressed);
        assert_eq!(
            pressed.background,
            Some(Background::Color(colors.surfaceContainer))
        );
        assert_eq!(pressed.text_color, colors.onControl);
        assert_eq!(pressed.border.color, colors.borderSubtle);
    }

    #[test]
    fn tonal_active_uses_control_hover_with_subtle_border() {
        let theme = dark_theme();
        let colors = DARK_PALETTE.colors;

        for status in [Status::Active, Status::Hovered, Status::Pressed] {
            let style = super::style(&theme, ButtonVariant::TonalActive, status);
            assert_eq!(
                style.background,
                Some(Background::Color(colors.controlHover)),
                "TonalActive must stay filled in status {status:?}"
            );
            assert_eq!(style.text_color, colors.onControlHover);
            assert_eq!(style.border.width, 1.0);
            assert_eq!(style.border.color, colors.borderSubtle);
        }
    }

    #[test]
    fn pill_matches_tonal_visuals() {
        let theme = dark_theme();
        let colors = DARK_PALETTE.colors;

        let active = super::style(&theme, ButtonVariant::Pill, Status::Active);
        assert_eq!(
            active.background,
            Some(Background::Color(colors.surfaceContainerHigh))
        );
        assert_eq!(active.text_color, colors.onControl);
        assert_eq!(active.border.width, 1.0);
        assert_eq!(active.border.color, colors.borderSubtle);

        let hovered = super::style(&theme, ButtonVariant::Pill, Status::Hovered);
        assert_eq!(
            hovered.background,
            Some(Background::Color(colors.controlHover))
        );
        assert_eq!(hovered.text_color, colors.onControlHover);

        let pressed = super::style(&theme, ButtonVariant::Pill, Status::Pressed);
        assert_eq!(
            pressed.background,
            Some(Background::Color(colors.surfaceContainer))
        );
        assert_eq!(pressed.text_color, colors.onControl);
    }

    #[test]
    fn pill_active_uses_primary_container() {
        let theme = dark_theme();
        let colors = DARK_PALETTE.colors;

        for status in [Status::Active, Status::Hovered, Status::Pressed] {
            let style = super::style(&theme, ButtonVariant::PillActive, status);
            assert_eq!(
                style.background,
                Some(Background::Color(colors.primaryContainer)),
                "PillActive must stay filled in status {status:?}"
            );
            assert_eq!(style.text_color, colors.secondary);
            assert_eq!(style.border.width, 0.0);
        }
    }

    #[test]
    fn icon_matches_tonal_visuals() {
        let theme = dark_theme();
        let colors = DARK_PALETTE.colors;

        let active = super::style(&theme, ButtonVariant::Icon, Status::Active);
        assert_eq!(
            active.background,
            Some(Background::Color(colors.surfaceContainerHigh))
        );
        assert_eq!(active.text_color, colors.onControl);
        assert_eq!(active.border.width, 1.0);
        assert_eq!(active.border.color, colors.borderSubtle);

        let hovered = super::style(&theme, ButtonVariant::Icon, Status::Hovered);
        assert_eq!(
            hovered.background,
            Some(Background::Color(colors.controlHover))
        );
        assert_eq!(hovered.text_color, colors.onControlHover);

        let pressed = super::style(&theme, ButtonVariant::Icon, Status::Pressed);
        assert_eq!(
            pressed.background,
            Some(Background::Color(colors.surfaceContainer))
        );
        assert_eq!(pressed.text_color, colors.onControl);
    }

    #[test]
    fn text_and_secondary_fills_unchanged() {
        let theme = dark_theme();
        let colors = DARK_PALETTE.colors;
        let text = DARK_PALETTE.text;

        let secondary_active = super::style(&theme, ButtonVariant::Secondary, Status::Active);
        assert_eq!(
            secondary_active.background,
            Some(Background::Color(colors.secondaryContainer))
        );
        assert_eq!(secondary_active.text_color, colors.onSecondaryContainer);

        let secondary_pressed = super::style(&theme, ButtonVariant::Secondary, Status::Pressed);
        assert_eq!(
            secondary_pressed.background,
            Some(Background::Color(colors.secondaryContainer))
        );
        assert_eq!(secondary_pressed.text_color, colors.onSecondaryContainer);

        let text_active = super::style(&theme, ButtonVariant::Text, Status::Active);
        assert_eq!(text_active.background, None);
        assert_eq!(text_active.text_color, text.body);

        let text_pressed = super::style(&theme, ButtonVariant::Text, Status::Pressed);
        assert_eq!(text_pressed.background, None);
        assert_eq!(text_pressed.text_color, text.body);

        let text_hovered = super::style(&theme, ButtonVariant::Text, Status::Hovered);
        assert_eq!(
            text_hovered.background,
            Some(Background::Color(colors.control))
        );
        assert_eq!(text_hovered.text_color, text.heading);
    }

    #[test]
    fn primary_secondary_text_are_borderless() {
        let theme = dark_theme();
        for variant in [
            ButtonVariant::Primary,
            ButtonVariant::Secondary,
            ButtonVariant::Text,
        ] {
            for status in [
                Status::Active,
                Status::Hovered,
                Status::Pressed,
                Status::Disabled,
            ] {
                let style = super::style(&theme, variant, status);
                assert_eq!(
                    style.border.width, 0.0,
                    "{variant:?} must have zero border width in status {status:?}"
                );
                assert_eq!(
                    style.border.color,
                    Color::TRANSPARENT,
                    "{variant:?} must have a transparent border in status {status:?}"
                );
            }
        }
    }

    #[test]
    fn disabled_uses_shared_control_and_muted_rule() {
        let theme = dark_theme();
        let colors = DARK_PALETTE.colors;

        for variant in [
            ButtonVariant::Primary,
            ButtonVariant::Secondary,
            ButtonVariant::Tonal,
            ButtonVariant::TonalActive,
            ButtonVariant::Text,
            ButtonVariant::Icon,
            ButtonVariant::Pill,
            ButtonVariant::PillActive,
        ] {
            let style = super::style(&theme, variant, Status::Disabled);
            assert_eq!(
                style.background,
                Some(Background::Color(colors.control)),
                "{variant:?} disabled fill must be the control token"
            );
            assert_eq!(
                style.text_color, DARK_PALETTE.text.muted,
                "{variant:?} disabled content must be text.muted"
            );
            assert_eq!(
                style.border.width, 0.0,
                "{variant:?} disabled border must be removed"
            );
        }
    }

    #[test]
    fn hero_controls_do_not_inherit_decorative_borders() {
        for mode in [
            crate::theme::ThemeMode::Dark,
            crate::theme::ThemeMode::Light,
        ] {
            let theme = crate::theme::theme(mode);
            for variant in [ButtonVariant::Tonal, ButtonVariant::Text] {
                for status in [
                    Status::Active,
                    Status::Hovered,
                    Status::Pressed,
                    Status::Disabled,
                ] {
                    assert_eq!(super::hero_glass(&theme, variant, status).border.width, 0.0);
                }
            }
        }
    }
}
