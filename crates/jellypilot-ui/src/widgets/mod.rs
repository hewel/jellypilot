//! Catalog style functions for JellyPilot's basic iced widgets.

pub mod artwork_blur;
pub mod artwork_grid;
pub mod artwork_progress;
pub mod badge;
pub mod button;
pub mod container;
pub mod control_button;
pub mod ellipsis_text;
pub mod embedded_player;
pub mod escape_input;
pub mod field;
pub mod focus_scope;
pub mod inert;
pub mod library;
pub mod motion;
pub mod notice_interaction;
pub mod player_bar;
pub mod poster_card;
pub mod rounded_image;
pub mod scrollable;
pub mod search_field;
pub mod settings;
pub mod sidebar;
pub mod skeleton;
pub mod sticky_navigation;
pub mod switch;
pub mod tracked_slider;
pub mod tv_focus;
pub mod tv_player;

pub use control_button::{control_button, ControlButton};
pub use poster_card::{poster_card, PosterCard};
pub use rounded_image::{card_top_radius, full_radius, rounded_image, RoundedImage};
pub use search_field::{search_field, SearchField};

/// Copies a child layout while preserving its parent bounds.
pub(crate) fn child_layout(
    layout: iced::advanced::Layout,
    tree: &iced::advanced::widget::Tree,
    index: usize,
) -> Option<iced::advanced::Layout> {
    layout
        .iter(&tree.children)
        .nth(index)
        .map(|(layout, _)| layout)
}
