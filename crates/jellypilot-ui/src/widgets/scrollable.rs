//! JellyPilot scrollable catalog styles.

use iced::widget::scrollable;
use iced::{Background, Border, Color, Theme};

use crate::tokens::{palette, SemanticColors, ThemePalette, TOKENS};

/// Resolves a scrollable interaction status to JellyPilot scrollbar chrome.
pub fn style(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let palette = palette(theme);
    let (horizontal_interactive, vertical_interactive) = axis_interactions(status);

    scrollable::Style {
        container: Default::default(),
        vertical_rail: rail(palette.colors, vertical_interactive),
        horizontal_rail: rail(palette.colors, horizontal_interactive),
        gap: Some(Background::Color(with_alpha(
            palette.colors.surfaceContainerLowest,
            0.4,
        ))),
        auto_scroll: auto_scroll(palette),
    }
}

fn rail(colors: SemanticColors, is_interactive: bool) -> scrollable::Rail {
    scrollable::Rail {
        background: Some(Background::Color(with_alpha(
            colors.surfaceContainerLowest,
            0.4,
        ))),
        border: Border {
            radius: TOKENS.radii.full.into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        scroller: scrollable::Scroller {
            background: Background::Color(scroller_background(colors, is_interactive)),
            border: Border {
                radius: TOKENS.radii.full.into(),
                color: Color::TRANSPARENT,
                width: 2.0,
            },
        },
    }
}

fn axis_interactions(status: scrollable::Status) -> (bool, bool) {
    match status {
        scrollable::Status::Active { .. } => (false, false),
        scrollable::Status::Hovered {
            is_horizontal_scrollbar_hovered,
            is_vertical_scrollbar_hovered,
            ..
        } => (
            is_horizontal_scrollbar_hovered,
            is_vertical_scrollbar_hovered,
        ),
        scrollable::Status::Dragged {
            is_horizontal_scrollbar_dragged,
            is_vertical_scrollbar_dragged,
            ..
        } => (
            is_horizontal_scrollbar_dragged,
            is_vertical_scrollbar_dragged,
        ),
    }
}

fn scroller_background(colors: SemanticColors, is_interactive: bool) -> Color {
    if is_interactive {
        colors.outline
    } else {
        colors.outlineVariant
    }
}

fn auto_scroll(palette: &ThemePalette) -> scrollable::AutoScroll {
    scrollable::AutoScroll {
        background: Background::Color(palette.colors.surface),
        border: Border {
            radius: TOKENS.radii.full.into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        shadow: palette.shadows.raised.iced(),
        icon: palette.colors.onSurfaceVariant,
    }
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color { a: alpha, ..color }
}

#[cfg(test)]
mod tests {
    use iced::widget::scrollable;
    use iced::Background;

    use super::style;
    use crate::tokens::DARK_PALETTE;

    #[test]
    fn horizontal_hover_does_not_restyle_vertical_scroller() {
        let theme = crate::theme::theme(crate::theme::ThemeMode::Dark);
        let styled = style(
            &theme,
            scrollable::Status::Hovered {
                is_horizontal_scrollbar_hovered: true,
                is_vertical_scrollbar_hovered: false,
                is_horizontal_scrollbar_disabled: false,
                is_vertical_scrollbar_disabled: false,
            },
        );

        assert_eq!(
            (
                styled.horizontal_rail.scroller.background,
                styled.vertical_rail.scroller.background,
            ),
            (
                Background::Color(DARK_PALETTE.colors.outline),
                Background::Color(DARK_PALETTE.colors.outlineVariant),
            )
        );
    }

    #[test]
    fn vertical_drag_does_not_restyle_horizontal_scroller() {
        let theme = crate::theme::theme(crate::theme::ThemeMode::Dark);
        let styled = style(
            &theme,
            scrollable::Status::Dragged {
                is_horizontal_scrollbar_dragged: false,
                is_vertical_scrollbar_dragged: true,
                is_horizontal_scrollbar_disabled: false,
                is_vertical_scrollbar_disabled: false,
            },
        );

        assert_eq!(
            (
                styled.horizontal_rail.scroller.background,
                styled.vertical_rail.scroller.background,
            ),
            (
                Background::Color(DARK_PALETTE.colors.outlineVariant),
                Background::Color(DARK_PALETTE.colors.outline),
            )
        );
    }
}
