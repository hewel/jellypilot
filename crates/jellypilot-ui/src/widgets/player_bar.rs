//! Catalog styles for the docked footer player.

use iced::widget::slider;
use iced::{Color, Theme};

use crate::tokens::{palette, TOKENS};

/// Footer timeline chrome; the view provides its 40px interaction region.
pub fn timeline(theme: &Theme, status: slider::Status) -> slider::Style {
    let colors = palette(theme).colors;
    let active = status != slider::Status::Active;
    let mut style = slider::default(theme, status);
    style.rail.width = if active { 6.0 } else { 4.0 };
    style.rail.backgrounds = (colors.primary.into(), colors.surfaceContainerHighest.into());
    style.rail.border.radius = TOKENS.radii.full.into();
    // Keep the handle geometry stable so hover does not move the played edge.
    style.handle.shape = slider::HandleShape::Circle { radius: 6.0 };
    style.handle.background = if active {
        colors.primary
    } else {
        Color::TRANSPARENT
    }
    .into();
    style
}
