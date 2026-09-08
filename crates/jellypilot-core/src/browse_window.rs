//! Sparse Browse metadata coverage for the measured Library Image demand window.
use std::ops::Range;

fn positive_finite(value: f32) -> Option<f32> {
    (value.is_finite() && value > 0.0).then_some(value)
}

/// Maps scroll geometry to the clamped display-index window the grid covers.
///
/// Expands the vertical viewport by one viewport length before and after,
/// rounds outward to complete rows, and clamps item indexes to `total`.
/// `offset_y` is signed and grid-local; `viewport_height` is the full parent
/// visible viewport height, including the portion before the grid.
/// A zero-height viewport or unusable row geometry produces an empty window.
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
    if total == 0 || columns == 0 || !offset_y.is_finite() {
        return 0..0;
    }
    let viewport_height = finite_non_negative(viewport_height);
    if viewport_height == 0.0 {
        return 0..0;
    }
    let columns = u32::try_from(columns).unwrap_or(u32::MAX);
    let first_row = ((offset_y - viewport_height).max(0.0) / row_height).floor() as u32;
    let end_row = ((offset_y + 2.0 * viewport_height) / row_height).ceil() as u32;
    let start = first_row.saturating_mul(columns).min(total);
    let end = end_row.saturating_mul(columns).min(total).max(start);
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

    #[test]
    fn display_window_covers_exact_prefetch_span_in_a_large_sparse_library() {
        let columns = 8;
        let row_height = 275.0;
        let offset = 275_050.0;
        let height = 900.0;
        let window = visible_display_range(offset, height, columns, row_height, 1_000_000);
        let first_y = (window.start / columns as u32) as f32 * row_height;
        let end_y = (window.end / columns as u32) as f32 * row_height;
        assert!(first_y <= offset - height && first_y + row_height > offset - height);
        assert!(end_y >= offset + 2.0 * height && end_y - row_height < offset + 2.0 * height);
        assert!(window.len() <= (3.0 * height / row_height).ceil() as usize * columns + columns);
    }

    #[test]
    fn display_window_clips_to_library_ends_and_partial_final_row() {
        assert_eq!(visible_display_range(0.0, 300.0, 4, 100.0, 401), 0..24);
        assert_eq!(
            visible_display_range(9_800.0, 300.0, 4, 100.0, 401),
            380..401
        );
        assert_eq!(
            visible_display_range(20_000.0, 300.0, 4, 100.0, 401),
            401..401
        );
    }

    #[test]
    fn display_window_resize_changes_both_prefetch_edges() {
        let small = visible_display_range(2_000.0, 100.0, 4, 100.0, 10_000);
        let large = visible_display_range(2_000.0, 400.0, 4, 100.0, 10_000);
        assert_eq!(small.start - large.start, 3 * 4);
        assert_eq!(large.end - small.end, 6 * 4);
        let wider = visible_display_range(2_000.0, 400.0, 8, 100.0, 10_000);
        assert_eq!(wider, large.start * 2..large.end * 2);
    }

    #[test]
    fn display_window_rejects_unusable_geometry() {
        for invalid in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert!(visible_display_range(250.0, invalid, 4, 100.0, 1_000).is_empty());
            assert!(visible_display_range(250.0, 300.0, 4, invalid, 1_000).is_empty());
        }
        assert!(visible_display_range(0.0, 300.0, 0, 100.0, 1_000).is_empty());
        assert!(visible_display_range(0.0, 300.0, 4, 100.0, 0).is_empty());
        for invalid_offset in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(visible_display_range(invalid_offset, 300.0, 4, 100.0, 1_000).is_empty());
        }
    }

    #[test]
    fn signed_grid_offset_preserves_the_parent_prefetch_distance() {
        assert_eq!(visible_display_range(-120.0, 300.0, 4, 100.0, 1_000), 0..20);
        assert_eq!(visible_display_range(-700.0, 300.0, 4, 100.0, 1_000), 0..0);
        for scroll_offset in [520.0, 920.0, 520.0] {
            let local_top = scroll_offset - 120.0;
            let range = visible_display_range(local_top, 300.0, 4, 100.0, 1_000);
            assert_eq!(range.start, ((local_top - 300.0) / 100.0) as u32 * 4);
            assert_eq!(range.end, ((local_top + 600.0) / 100.0) as u32 * 4);
        }
    }
}
