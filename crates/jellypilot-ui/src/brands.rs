//! Vendored Jellyfin/Emby brand marks for account identity badges.
//!
//! Unlike the [`crate::icons`] catalog these render with their original brand
//! colors; tinting them would misrepresent the marks.

use std::sync::LazyLock;

use iced::widget::svg::{Handle, Svg};
use iced::{Length, Theme};

/// Server-type brand marks vendored under `assets/brands/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Brand {
    Jellyfin,
    Emby,
}

const BRAND_COUNT: usize = 2;
static BRAND_HANDLES: LazyLock<[Handle; BRAND_COUNT]> = LazyLock::new(|| {
    [
        Handle::from_memory(include_bytes!("../assets/brands/jellyfin.svg").as_slice()),
        Handle::from_memory(include_bytes!("../assets/brands/emby.svg").as_slice()),
    ]
});

impl Brand {
    const fn index(self) -> usize {
        match self {
            Self::Jellyfin => 0,
            Self::Emby => 1,
        }
    }

    fn handle(self) -> Handle {
        BRAND_HANDLES[self.index()].clone()
    }
}

/// Creates an iced `Svg` widget showing the brand mark at `px` logical size,
/// keeping the mark's original colors.
pub fn brand_svg<'a>(brand: Brand, px: f32) -> Svg<'a, Theme> {
    Svg::new(brand.handle())
        .width(Length::Fixed(px))
        .height(Length::Fixed(px))
}
