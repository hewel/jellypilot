//! Boolean switch using the shared keyboard-focusable control interaction.

use iced::widget::{button, container, space};
use iced::{Alignment, Border, Length, Theme};

use crate::icons::IconControlState;
use crate::tokens::{palette, TOKENS};
use crate::variants::ButtonVariant;

use super::control_button::{control_button_content, ControlButton};

/// Creates a 40 × 22 switch with an 18px knob and a 40px-high hit target.
/// The caller supplies the toggle message and an adjacent descriptive label.
pub fn switch<'a, Message: Clone + 'a>(enabled: bool) -> ControlButton<'a, Message> {
    control_button_content(
        move |state| {
            let disabled = state == IconControlState::Disabled;
            let knob = container(space()).width(18).height(18).style(move |theme| {
                let palette = palette(theme);
                let color = if disabled {
                    palette.text.muted
                } else if enabled {
                    palette.colors.onPrimary
                } else {
                    palette.colors.onControl
                };
                container::Style {
                    background: Some(color.into()),
                    border: Border::default().rounded(TOKENS.radii.full),
                    ..container::Style::default()
                }
            });
            container(knob)
                .width(40)
                .height(22)
                .padding(2)
                .align_x(if enabled {
                    Alignment::End
                } else {
                    Alignment::Start
                })
                .style(move |theme| {
                    let colors = palette(theme).colors;
                    let background = if disabled {
                        colors.control
                    } else if enabled {
                        colors.primary
                    } else if state == IconControlState::Hovered {
                        colors.controlHover
                    } else {
                        colors.surfaceContainerHighest
                    };
                    container::Style {
                        background: Some(background.into()),
                        border: Border::default().rounded(TOKENS.radii.full),
                        ..container::Style::default()
                    }
                })
                .into()
        },
        ButtonVariant::Text,
    )
    .padding([9, 0])
    .width(Length::Fixed(40.0))
    .min_height(40.0)
    .style(style)
}

fn style(_theme: &Theme, _variant: ButtonVariant, _status: button::Status) -> button::Style {
    button::Style {
        border: Border::default().rounded(TOKENS.radii.md),
        ..button::Style::default()
    }
}
