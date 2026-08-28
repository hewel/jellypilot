//! JellyPilot scrollable catalog styles.

use iced::widget::scrollable;
use iced::{Background, Border, Color, Theme};

use crate::tokens::TOKENS;

/// Resolves a scrollable interaction status to JellyPilot scrollbar chrome.
pub fn style(_theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let (horizontal_interactive, vertical_interactive) = axis_interactions(status);

    scrollable::Style {
        container: Default::default(),
        vertical_rail: rail(vertical_interactive),
        horizontal_rail: rail(horizontal_interactive),
        gap: Some(Background::Color(with_alpha(
            TOKENS.colors.surfaceContainerLowest,
            0.4,
        ))),
        auto_scroll: auto_scroll(),
    }
}

fn rail(is_interactive: bool) -> scrollable::Rail {
    scrollable::Rail {
        background: Some(Background::Color(with_alpha(
            TOKENS.colors.surfaceContainerLowest,
            0.4,
        ))),
        border: Border {
            radius: TOKENS.radii.full.into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        scroller: scrollable::Scroller {
            background: Background::Color(scroller_background(is_interactive)),
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

fn scroller_background(is_interactive: bool) -> Color {
    if is_interactive {
        TOKENS.colors.outline
    } else {
        with_alpha(TOKENS.colors.outlineVariant, 0.8)
    }
}

fn auto_scroll() -> scrollable::AutoScroll {
    scrollable::AutoScroll {
        background: Background::Color(TOKENS.colors.surface),
        border: Border {
            radius: TOKENS.radii.full.into(),
            color: TOKENS.colors.outlineVariant,
            width: 1.0,
        },
        shadow: TOKENS.shadows.sm.iced(),
        icon: TOKENS.colors.onSurfaceVariant,
    }
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color { a: alpha, ..color }
}

#[cfg(test)]
mod tests {
    use iced::widget::scrollable;
    use iced::Background;

    use super::{style, with_alpha};
    use crate::tokens::TOKENS;

    #[test]
    fn horizontal_hover_does_not_restyle_vertical_scroller() {
        let theme = crate::theme::theme();
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
                Background::Color(TOKENS.colors.outline),
                Background::Color(with_alpha(TOKENS.colors.outlineVariant, 0.8)),
            )
        );
    }

    #[test]
    fn vertical_drag_does_not_restyle_horizontal_scroller() {
        let theme = crate::theme::theme();
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
                Background::Color(with_alpha(TOKENS.colors.outlineVariant, 0.8)),
                Background::Color(TOKENS.colors.outline),
            )
        );
    }
}
