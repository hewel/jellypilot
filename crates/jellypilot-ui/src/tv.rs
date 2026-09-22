//! Scoped living-room geometry and focus styles. Desktop tokens remain unchanged.

use iced::widget::button::{Status, Style};
use iced::widget::container;
use iced::{Background, Border, Color, Theme};

use crate::tokens::{ThemePalette, DARK_PALETTE, TOKENS};

pub const TITLE: f32 = 40.0;
pub const SECTION: f32 = 32.0;
pub const BODY: f32 = 24.0;
pub const META: f32 = 22.0;
pub const SAFE_X: f32 = 96.0;
pub const SAFE_Y: f32 = 60.0;
pub const RAIL: f32 = 288.0;
pub const CONTENT_INSET: f32 = 48.0;
pub const CONTROL: f32 = 64.0;
pub const GAP: f32 = 24.0;
pub const FOCUS_SURFACE: Color = Color::from_rgb8(0xe8, 0xe9, 0xed);
pub const ON_FOCUS: Color = Color::from_rgb8(0x17, 0x18, 0x1c);
pub const ON_FOCUS_SECONDARY: Color = Color::from_rgb8(0x49, 0x4b, 0x54);
pub const FOCUS_EDGE: Color = FOCUS_SURFACE;
pub const FOCUS_WIDTH: f32 = 2.0;
pub const FOCUS_COLOR: Color = FOCUS_SURFACE;
pub const FOCUS_DURATION: std::time::Duration = std::time::Duration::from_millis(175);
pub const POSTER_SCALE: f32 = 1.03;
pub const POSTER_CLEARANCE: f32 = 18.0;
pub const PALETTE: ThemePalette = DARK_PALETTE;

/// Scale the 1920-pixel reference consistently with the current logical viewport.
pub fn scale(width: f32) -> f32 {
    (width / 1920.0).clamp(0.5, 2.0)
}

/// Immediate focus style for callers that do not animate their child content.
pub fn button(
    palette: ThemePalette,
    focused: bool,
    selected: bool,
) -> impl Fn(&Theme, Status) -> Style {
    button_progress(palette, if focused { 1.0 } else { 0.0 }, selected)
}

pub fn foreground(palette: ThemePalette, progress: f32, selected: bool) -> Color {
    crate::widgets::motion::lerp_color(
        if selected {
            palette.colors.secondary
        } else {
            palette.text.heading
        },
        ON_FOCUS,
        progress,
    )
}

pub fn secondary_foreground(palette: ThemePalette, progress: f32) -> Color {
    crate::widgets::motion::lerp_color(palette.text.metadata, ON_FOCUS_SECONDARY, progress)
}

/// Selection restores its indigo fill after focus moves away; focus never changes its value.
pub fn button_progress(
    palette: ThemePalette,
    progress: f32,
    selected: bool,
) -> impl Fn(&Theme, Status) -> Style {
    move |_, status| Style {
        background: Some(Background::Color(crate::widgets::motion::lerp_color(
            if selected {
                palette.colors.primaryContainer
            } else if matches!(status, Status::Hovered | Status::Pressed) {
                palette.colors.controlHover
            } else {
                palette.colors.control
            },
            FOCUS_SURFACE,
            if status == Status::Disabled {
                0.0
            } else {
                progress
            },
        ))),
        text_color: foreground(
            palette,
            if status == Status::Disabled {
                0.0
            } else {
                progress
            },
            selected,
        ),
        border: Border {
            radius: TOKENS.radii.xl.into(),
            ..Border::default()
        },
        ..Style::default()
    }
}

pub fn poster(progress: f32) -> impl Fn(&Theme, Status) -> Style {
    move |_, _| Style {
        background: Some(PALETTE.colors.surfaceContainer.into()),
        text_color: PALETTE.text.heading,
        border: Border {
            color: FOCUS_EDGE.scale_alpha(progress),
            width: FOCUS_WIDTH,
            radius: TOKENS.radii.xl.into(),
            ..Border::default()
        },
        ..Style::default()
    }
}

pub fn panel(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(PALETTE.colors.surfaceContainerHigh.into()),
        text_color: Some(PALETTE.text.body),
        border: Border {
            radius: TOKENS.radii.xl.into(),
            ..Border::default()
        },
        ..container::Style::default()
    }
}

pub fn canvas(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(PALETTE.colors.background.into()),
        text_color: Some(PALETTE.text.body),
        ..container::Style::default()
    }
}

pub fn scrim(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(PALETTE.colors.background.scale_alpha(0.55).into()),
        ..container::Style::default()
    }
}

pub fn rail(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(PALETTE.colors.sidebarBg.into()),
        text_color: Some(PALETTE.text.body),
        ..container::Style::default()
    }
}
