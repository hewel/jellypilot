//! Bundled typefaces used by the JellyPilot iced frontend.

use iced::Font;

/// Inter variable font bytes (optical size and weight axes).
pub const INTER: &[u8] = include_bytes!("../assets/fonts/Inter-Variable.ttf");
/// Space Grotesk variable font bytes (weight axis).
pub const SPACE_GROTESK: &[u8] = include_bytes!("../assets/fonts/SpaceGrotesk-Variable.ttf");

/// The default body typeface registered by the application shell.
pub const INTER_FONT: Font = Font::with_name("Inter");
/// The display typeface available for prominent headings.
pub const SPACE_GROTESK_FONT: Font = Font::with_name("Space Grotesk");

/// Returns every bundled typeface for registration with iced.
pub const fn fonts() -> [&'static [u8]; 2] {
    [INTER, SPACE_GROTESK]
}
