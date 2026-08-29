//! JellyPilot's dark-only iced design system.
//!
//! The crate ports the canonical Panda tokens and basic component variants so
//! native screens share the same visual vocabulary as the Tauri frontend.

pub mod fonts;
pub mod icons;
pub mod overlay;
pub mod theme;
pub mod tokens;
pub mod variants;
pub mod widgets;

pub use icons::{
    icon, icon_for_variant, icon_for_variant_disabled, icon_for_variant_status, icon_sized,
    icon_with_color, Icon, IconSize, DEFAULT_ICON_SIZE, ICON_SIZE_2XL, ICON_SIZE_LG, ICON_SIZE_MD,
    ICON_SIZE_SM, ICON_SIZE_XL, ICON_SIZE_XS,
};
pub use widgets::poster_card::{poster_card, PosterCard};
pub use widgets::rounded_image::{card_top_radius, full_radius, rounded_image, RoundedImage};
