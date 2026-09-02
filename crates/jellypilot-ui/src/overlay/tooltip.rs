//! Tokenized convenience wrappers around iced's built-in tooltip widget.

use std::time::Duration;

use iced::widget::{container, text, tooltip as iced_tooltip};
use iced::Element;

use super::style;
use super::Placement;
use crate::tokens::TOKENS;

const TOOLTIP_DELAY: Duration = TOKENS.durations.ms200;
const TOOLTIP_MAX_WIDTH: f32 = TOKENS.spacing.tooltip_max_width;

/// Appearance and behavior of a JellyPilot tooltip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TooltipOptions {
    pub placement: Placement,
    pub delay: Duration,
    pub enabled: bool,
    pub max_width: f32,
}

impl Default for TooltipOptions {
    fn default() -> Self {
        Self {
            placement: Placement::Above,
            delay: TOOLTIP_DELAY,
            enabled: true,
            max_width: TOOLTIP_MAX_WIDTH,
        }
    }
}

/// Wraps a trigger with a tokenized text tooltip.
pub fn tooltip<'a, Message: 'a>(
    trigger: impl Into<Element<'a, Message>>,
    content: impl Into<String>,
    options: TooltipOptions,
) -> Element<'a, Message> {
    tooltip_element(
        trigger,
        text(content.into()).size(TOKENS.font_sizes.s12),
        options,
    )
}

/// Wraps a trigger with a caller-composed, tokenized tooltip.
pub fn tooltip_element<'a, Message: 'a>(
    trigger: impl Into<Element<'a, Message>>,
    content: impl Into<Element<'a, Message>>,
    options: TooltipOptions,
) -> Element<'a, Message> {
    let trigger = trigger.into();

    if !options.enabled {
        return trigger;
    }

    iced_tooltip(
        trigger,
        container(content).max_width(options.max_width),
        iced_position(options.placement),
    )
    .delay(options.delay)
    .gap(TOKENS.spacing.s1_5)
    .padding(TOKENS.spacing.s2)
    .snap_within_viewport(true)
    .style(style::tooltip_surface)
    .into()
}

const fn iced_position(placement: Placement) -> iced_tooltip::Position {
    match placement {
        Placement::Above => iced_tooltip::Position::Top,
        Placement::Below => iced_tooltip::Position::Bottom,
        Placement::Start => iced_tooltip::Position::Left,
        Placement::End => iced_tooltip::Position::Right,
    }
}

#[cfg(test)]
mod tests {
    use iced::widget::tooltip;

    use super::iced_position;
    use crate::overlay::Placement;

    #[test]
    fn every_overlay_placement_maps_to_the_builtin_tooltip() {
        assert_eq!(iced_position(Placement::Above), tooltip::Position::Top);
        assert_eq!(iced_position(Placement::Below), tooltip::Position::Bottom);
        assert_eq!(iced_position(Placement::Start), tooltip::Position::Left);
        assert_eq!(iced_position(Placement::End), tooltip::Position::Right);
    }
}
