//! Display-free Library Image load orchestration: visibility classification
//! and a visible-first load plan.
//!
//! Surfaces classify each Library Image as visible or offscreen with the pure
//! helpers below, submit [`PlannedArtworkLoad`] items, and receive a plan that
//! loads the visible lane first with stable order inside each lane. The
//! frontend executes the plan as a stream of per-image
//! [`ArtworkLoadCompletion`] settlements.

use jellypilot_media_server::artwork::ArtworkSizeClass;
use std::ops::Range;

use crate::artwork_binder::ArtworkSlot;

/// One Library Image load submitted by a surface, classified by visibility
/// and the render-side decode bucket its Library Image Raster targets.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedArtworkLoad {
    pub slot: ArtworkSlot,
    pub image_id: String,
    pub size_class: ArtworkSizeClass,
    pub visible: bool,
}

/// One settled Library Image load delivered back to its surface.
#[derive(Clone, Debug)]
pub struct ArtworkLoadCompletion<R> {
    pub slot: ArtworkSlot,
    pub image_id: String,
    pub result: R,
}

/// Orders loads visible-first while preserving submission order within each
/// lane, so a freshly opened page paints its visible Library Images before
/// offscreen work occupies the loader.
#[must_use]
pub fn plan_artwork_loads(loads: Vec<PlannedArtworkLoad>) -> Vec<PlannedArtworkLoad> {
    let mut visible = Vec::new();
    let mut offscreen = Vec::new();
    for load in loads {
        if load.visible {
            visible.push(load);
        } else {
            offscreen.push(load);
        }
    }
    visible.extend(offscreen);
    visible
}

/// Reports whether the grid cell at `index` intersects the vertical viewport.
///
/// Mirrors the visible row window the grid renders (without overscan): the
/// first visible row contains `offset_y` and the last visible row contains
/// `offset_y + viewport_height`. A zero-height viewport or unusable row
/// geometry classifies every cell as offscreen.
#[must_use]
pub fn grid_cell_visible(
    index: usize,
    columns: usize,
    offset_y: f32,
    viewport_height: f32,
    row_height: f32,
) -> bool {
    let Some(row_height) = positive_finite(row_height) else {
        return false;
    };
    let viewport_height = finite_non_negative(viewport_height);
    if viewport_height == 0.0 {
        return false;
    }
    let offset_y = finite_non_negative(offset_y);
    let row = index / columns.max(1);
    let first_visible_row = (offset_y / row_height).floor() as usize;
    let past_visible_row = ((offset_y + viewport_height) / row_height).ceil() as usize;
    row >= first_visible_row && row < past_visible_row
}

/// Counts how many fixed-width cards in a horizontal section row intersect a
/// viewport of `viewport_width`. A partially visible card counts; an empty
/// viewport or unusable card geometry counts nothing.
#[must_use]
pub fn visible_row_cards(viewport_width: f32, card_width: f32, gap: f32) -> usize {
    let Some(card_width) = positive_finite(card_width) else {
        return 0;
    };
    let viewport_width = finite_non_negative(viewport_width);
    if viewport_width == 0.0 {
        return 0;
    }
    let stride = card_width + finite_non_negative(gap);
    ((viewport_width / stride).ceil() as usize).max(1)
}

fn positive_finite(value: f32) -> Option<f32> {
    (value.is_finite() && value > 0.0).then_some(value)
}

/// Extra grid rows included in the loading window beyond each viewport edge.
pub const BROWSE_WINDOW_MARGIN_ROWS: u32 = 2;

/// Maps scroll geometry to the clamped display-index window the grid covers.
///
/// Rows come from dividing the viewport span by the grid row height; item
/// indexes are rows times the column count, expanded by
/// [`BROWSE_WINDOW_MARGIN_ROWS`] on each side and clamped to `total`.
#[must_use]
pub fn visible_display_range(
    offset_y: f32,
    viewport_height: f32,
    columns: usize,
    row_height: f32,
    total: u32,
) -> Range<u32> {
    let Some(row_height) = positive_finite(row_height) else {
        return 0..0;
    };
    if total == 0 || columns == 0 {
        return 0..0;
    }
    let offset_y = finite_non_negative(offset_y);
    let viewport_height = finite_non_negative(viewport_height);
    let columns = u32::try_from(columns).unwrap_or(u32::MAX);
    let first_row = (offset_y / row_height).floor() as u32;
    let end_row = ((offset_y + viewport_height) / row_height).ceil() as u32;
    let start = first_row
        .saturating_sub(BROWSE_WINDOW_MARGIN_ROWS)
        .saturating_mul(columns)
        .min(total);
    let end = end_row
        .saturating_add(BROWSE_WINDOW_MARGIN_ROWS)
        .saturating_mul(columns)
        .min(total)
        .max(start);
    start..end
}

fn finite_non_negative(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(slot: u64, visible: bool) -> PlannedArtworkLoad {
        PlannedArtworkLoad {
            slot: ArtworkSlot::for_test(slot),
            image_id: format!("image-{slot}"),
            size_class: ArtworkSizeClass::Card,
            visible,
        }
    }

    #[test]
    fn plan_loads_visible_lane_first_with_stable_order_inside_each_lane() {
        let plan = plan_artwork_loads(vec![
            load(1, false),
            load(2, true),
            load(3, false),
            load(4, true),
            load(5, false),
        ]);

        assert_eq!(
            plan,
            vec![
                load(2, true),
                load(4, true),
                load(1, false),
                load(3, false),
                load(5, false)
            ]
        );
    }

    #[test]
    fn plan_handles_empty_and_single_lane_inputs() {
        assert!(plan_artwork_loads(Vec::new()).is_empty());

        let all_visible = plan_artwork_loads(vec![load(1, true), load(2, true)]);
        assert_eq!(all_visible, vec![load(1, true), load(2, true)]);

        let all_offscreen = plan_artwork_loads(vec![load(1, false), load(2, false)]);
        assert_eq!(all_offscreen, vec![load(1, false), load(2, false)]);
    }

    #[test]
    fn grid_classification_marks_every_cell_offscreen_for_an_empty_viewport() {
        for index in 0..12 {
            assert!(!grid_cell_visible(index, 4, 0.0, 0.0, 200.0));
        }
    }

    #[test]
    fn grid_classification_includes_a_partially_visible_first_row() {
        // Half of row 0 plus one full row of height 200 fit the viewport.
        assert!(grid_cell_visible(0, 4, 100.0, 200.0, 200.0));
        assert!(grid_cell_visible(7, 4, 100.0, 200.0, 200.0));
        assert!(!grid_cell_visible(8, 4, 100.0, 200.0, 200.0));
    }

    #[test]
    fn grid_classification_follows_the_scroll_offset() {
        assert!(!grid_cell_visible(0, 4, 400.0, 200.0, 200.0));
        assert!(grid_cell_visible(8, 4, 400.0, 200.0, 200.0));
        assert!(grid_cell_visible(11, 4, 400.0, 200.0, 200.0));
        assert!(!grid_cell_visible(12, 4, 400.0, 200.0, 200.0));
    }

    #[test]
    fn grid_classification_rejects_unusable_geometry() {
        assert!(!grid_cell_visible(0, 4, 0.0, 200.0, 0.0));
        assert!(!grid_cell_visible(0, 4, 0.0, 200.0, f32::NAN));
        assert!(!grid_cell_visible(0, 4, 0.0, f32::NAN, 200.0));
        // Zero columns behaves as a single column rather than dividing by zero.
        assert!(grid_cell_visible(1, 0, 0.0, 500.0, 200.0));
        assert!(!grid_cell_visible(3, 0, 0.0, 500.0, 200.0));
    }

    #[test]
    fn row_classification_counts_nothing_for_an_empty_viewport() {
        assert_eq!(visible_row_cards(0.0, 160.0, 16.0), 0);
        assert_eq!(visible_row_cards(f32::NAN, 160.0, 16.0), 0);
    }

    #[test]
    fn row_classification_counts_partial_cards() {
        // Card stride is 176; a sliver of the next card still counts.
        assert_eq!(visible_row_cards(176.0, 160.0, 16.0), 1);
        assert_eq!(visible_row_cards(177.0, 160.0, 16.0), 2);
        assert_eq!(visible_row_cards(80.0, 160.0, 16.0), 1);
    }

    #[test]
    fn row_classification_rejects_unusable_card_geometry() {
        assert_eq!(visible_row_cards(500.0, 0.0, 16.0), 0);
        assert_eq!(visible_row_cards(500.0, f32::INFINITY, 16.0), 0);
    }

    #[test]
    fn home_section_classification_uses_row_visibility_per_section() {
        // 1352 px content width, 240 px continue-watching thumbs, 16 px gap.
        let visible = visible_row_cards(1352.0, 240.0, 16.0);
        assert_eq!(visible, 6);
        let loads = (0..10)
            .map(|index| load(index as u64, index < visible))
            .collect();
        let plan = plan_artwork_loads(loads);
        assert!(plan.iter().take(6).all(|planned| planned.visible));
        assert!(plan.iter().skip(6).all(|planned| !planned.visible));
        let ordered = plan.iter().map(|planned| planned.slot).collect::<Vec<_>>();
        assert_eq!(
            ordered,
            (0..10).map(ArtworkSlot::for_test).collect::<Vec<_>>()
        );
    }

    #[test]
    fn detail_classification_plans_poster_and_backdrop_before_episodes() {
        let loads = vec![
            load(1, true),  // poster
            load(2, true),  // backdrop
            load(3, false), // season episode
            load(4, false), // season episode
        ];

        let plan = plan_artwork_loads(loads);

        assert_eq!(
            plan,
            vec![load(1, true), load(2, true), load(3, false), load(4, false)]
        );
    }

    #[test]
    fn visible_display_range_maps_scroll_geometry_to_item_indexes() {
        // 8 columns, 275 px rows.
        let columns = 8;
        let row_height = 275.0;

        // Top of the grid: four visible rows plus two margin rows below.
        assert_eq!(
            visible_display_range(0.0, 900.0, columns, row_height, 264),
            0..48
        );
        // Middle: rows 10..14 visible, expanded to rows 8..16.
        assert_eq!(
            visible_display_range(2750.0, 900.0, columns, row_height, 264),
            64..128
        );
        // Near the end the window clamps to the total.
        assert_eq!(
            visible_display_range(8800.0, 900.0, columns, row_height, 264),
            240..264
        );
        // A zero-height viewport still covers its margin rows.
        assert_eq!(
            visible_display_range(0.0, 0.0, columns, row_height, 264),
            0..16
        );
        // An empty library yields an empty window.
        assert_eq!(
            visible_display_range(0.0, 900.0, columns, row_height, 0),
            0..0
        );
    }

    #[test]
    fn visible_display_range_sanitizes_degenerate_inputs() {
        let columns = 8;
        let row_height = 275.0;

        // Non-finite or negative geometry falls back to the grid origin.
        assert_eq!(
            visible_display_range(f32::NAN, 900.0, columns, row_height, 264),
            0..48
        );
        assert_eq!(
            visible_display_range(2750.0, f32::INFINITY, columns, row_height, 264),
            64..96
        );
        assert_eq!(
            visible_display_range(-50.0, 900.0, columns, row_height, 264),
            0..48
        );

        // Degenerate metrics cannot map rows, so the window is empty.
        assert_eq!(visible_display_range(0.0, 900.0, columns, 0.0, 264), 0..0);
        assert_eq!(visible_display_range(0.0, 900.0, 0, row_height, 264), 0..0);
    }
}
