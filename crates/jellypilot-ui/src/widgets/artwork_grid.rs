//! Viewport-sliced artwork grid built from normal iced layout widgets.
//!
//! Renders a full-height sparse range where cells look up items by global index.
//! This allows the browse grid to represent the total item count with top and
//! bottom spacers while only generating iced widget trees for the visible and
//! overscanned rows.

use iced::widget::{container, scrollable, Column, Row, Space};
use iced::{Element, Length};

use crate::tokens::TOKENS;

/// Poster width used when the available width cannot yet be measured.
pub const MIN_ARTWORK_CELL_WIDTH: f32 = TOKENS.spacing.x9l;
/// JellyPilot poster height divided by poster width.
pub const POSTER_ASPECT_RATIO: f32 = 1.5;

const COLUMN_GAP: f32 = TOKENS.spacing.s4;
const ROW_GAP: f32 = TOKENS.spacing.s4;
const MIN_OVERSCAN_ROWS: usize = 6;
const MAX_OVERSCAN_ROWS: usize = 18;

/// The part of a grid intersecting its parent scroll viewport.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArtworkGridViewport {
    pub offset_y: f32,
    pub height: f32,
}

impl ArtworkGridViewport {
    /// Converts an iced viewport to grid-local geometry.
    ///
    /// `grid_scroll_margin` is the grid's vertical offset in the scrollable
    /// content. Pass zero when the grid is the first child.
    #[must_use]
    pub fn from_scrollable(viewport: scrollable::Viewport, grid_scroll_margin: f32) -> Self {
        Self::from_scroll_geometry(
            viewport.absolute_offset().y,
            viewport.bounds().height,
            grid_scroll_margin,
        )
    }

    /// Converts raw scroll geometry to grid-local coordinates.
    #[must_use]
    pub fn from_scroll_geometry(
        viewport_offset: f32,
        viewport_height: f32,
        grid_scroll_margin: f32,
    ) -> Self {
        let margin = finite_non_negative(grid_scroll_margin);
        let viewport_end = viewport_offset + viewport_height;
        let visible_start = viewport_offset.max(margin);

        Self {
            offset_y: (viewport_offset - margin).max(0.0),
            height: (viewport_end - visible_start).clamp(0.0, viewport_height),
        }
    }
}

/// Fixed metrics derived from the measured grid width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArtworkGridMetrics {
    pub columns: usize,
    pub cell_width: f32,
    pub cell_height: f32,
    pub row_height: f32,
}

impl ArtworkGridMetrics {
    /// Measures the responsive columns and fixed poster cell geometry.
    /// Metrics for poster cards with a copy area below the artwork: the
    /// artwork keeps the exact 2:3 poster aspect and `copy_height` adds room
    /// for title/caption lines beneath it.
    #[must_use]
    pub fn for_cards(available_width: f32, copy_height: f32) -> Self {
        let mut metrics = Self::for_width(available_width);
        metrics.cell_height += copy_height.max(0.0);
        metrics.row_height = metrics.cell_height + ROW_GAP;
        metrics
    }

    #[must_use]
    pub fn for_width(available_width: f32) -> Self {
        let width = if available_width.is_finite() && available_width > 0.0 {
            available_width
        } else {
            MIN_ARTWORK_CELL_WIDTH
        };
        let columns = (((width + COLUMN_GAP) / (MIN_ARTWORK_CELL_WIDTH + COLUMN_GAP)).floor()
            as usize)
            .max(1);
        let cell_width =
            ((width - COLUMN_GAP * (columns.saturating_sub(1) as f32)) / columns as f32).max(0.0);
        let cell_height = cell_width * POSTER_ASPECT_RATIO;

        Self {
            columns,
            cell_width,
            cell_height,
            row_height: cell_height + ROW_GAP,
        }
    }
}

/// Renders the visible and overscanned rows of a responsive artwork grid.
///
/// The parent stores the latest [`scrollable::Viewport`] received from
/// [`scrollable::Scrollable::on_scroll`], converts it with
/// [`ArtworkGridViewport::from_scrollable`], and supplies the measured grid
/// width. Cells look up items by global item index and may contain any normal
/// iced widgets.
pub fn artwork_grid<'a, Message, Builder>(
    item_count: usize,
    metrics: ArtworkGridMetrics,
    viewport: ArtworkGridViewport,
    cell_builder: Builder,
) -> Element<'a, Message>
where
    Message: 'a,
    Builder: Fn(usize) -> Element<'a, Message>,
{
    let row_count = item_count.div_ceil(metrics.columns);
    let window = row_window(
        row_count,
        viewport.offset_y,
        viewport.height,
        metrics.row_height,
    );
    let mut content: Column<'a, Message> = Column::new()
        .width(Length::Fill)
        .push(spacer(window.top_spacer));

    for row_index in window.start..window.end {
        let item_start = row_index * metrics.columns;
        let item_end = (item_start + metrics.columns).min(item_count);
        let row_height = if row_index + 1 == row_count {
            metrics.cell_height
        } else {
            metrics.row_height
        };
        let mut row = Row::new()
            .spacing(COLUMN_GAP)
            .width(Length::Fill)
            .height(row_height);

        for index in item_start..item_end {
            row = row.push(
                container(cell_builder(index))
                    .width(metrics.cell_width)
                    .height(metrics.cell_height),
            );
        }

        content = content.push(row);
    }

    content.push(spacer(window.bottom_spacer)).into()
}

fn spacer<'a, Message: 'a>(height: f32) -> Element<'a, Message> {
    container(Space::new()).height(height).into()
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct RowWindow {
    visible_start: usize,
    visible_end: usize,
    start: usize,
    end: usize,
    top_spacer: f32,
    bottom_spacer: f32,
}

fn row_window(row_count: usize, offset_y: f32, viewport_height: f32, row_height: f32) -> RowWindow {
    if row_count == 0 {
        return RowWindow {
            visible_start: 0,
            visible_end: 0,
            start: 0,
            end: 0,
            top_spacer: 0.0,
            bottom_spacer: 0.0,
        };
    }

    let row_height = if row_height.is_finite() && row_height > 0.0 {
        row_height
    } else {
        1.0
    };
    let offset_y = finite_non_negative(offset_y);
    let viewport_height = finite_non_negative(viewport_height);
    let visible_start = ((offset_y / row_height).floor() as usize).min(row_count);
    let visible_end = (((offset_y + viewport_height) / row_height).ceil() as usize)
        .max(visible_start)
        .min(row_count);
    let overscan = overscan_rows(viewport_height, row_height);
    let start = visible_start.saturating_sub(overscan);
    let end = visible_end.saturating_add(overscan).min(row_count);

    let remaining_rows = row_count.saturating_sub(end);

    RowWindow {
        visible_start,
        visible_end,
        start,
        end,
        top_spacer: start as f32 * row_height,
        bottom_spacer: if remaining_rows == 0 {
            0.0
        } else {
            (remaining_rows as f32 * row_height - ROW_GAP).max(0.0)
        },
    }
}

fn overscan_rows(viewport_height: f32, row_height: f32) -> usize {
    if !viewport_height.is_finite()
        || viewport_height <= 0.0
        || !row_height.is_finite()
        || row_height <= 0.0
    {
        return MIN_OVERSCAN_ROWS;
    }

    ((viewport_height / row_height).ceil() as usize)
        .saturating_mul(2)
        .clamp(MIN_OVERSCAN_ROWS, MAX_OVERSCAN_ROWS)
}

fn finite_non_negative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use iced::Element;

    use super::{
        artwork_grid, overscan_rows, row_window, ArtworkGridMetrics, ArtworkGridViewport,
        RowWindow, MAX_OVERSCAN_ROWS, MIN_OVERSCAN_ROWS, ROW_GAP,
    };

    const ROW_HEIGHT: f32 = 100.0;

    #[test]
    fn offset_zero_renders_visible_rows_and_forward_overscan() {
        assert_eq!(
            row_window(100, 0.0, 300.0, ROW_HEIGHT),
            RowWindow {
                visible_start: 0,
                visible_end: 3,
                start: 0,
                end: 9,
                top_spacer: 0.0,
                bottom_spacer: 9_084.0,
            }
        );
    }

    #[test]
    fn middle_offset_overscans_both_sides() {
        assert_eq!(
            row_window(100, 2_000.0, 300.0, ROW_HEIGHT),
            RowWindow {
                visible_start: 20,
                visible_end: 23,
                start: 14,
                end: 29,
                top_spacer: 1_400.0,
                bottom_spacer: 7_084.0,
            }
        );
    }

    #[test]
    fn end_offset_clamps_window_and_bottom_spacer() {
        assert_eq!(
            row_window(100, 9_700.0, 300.0, ROW_HEIGHT),
            RowWindow {
                visible_start: 97,
                visible_end: 100,
                start: 91,
                end: 100,
                top_spacer: 9_100.0,
                bottom_spacer: 0.0,
            }
        );
    }

    #[test]
    fn viewport_resize_recomputes_visible_and_overscan_rows() {
        let small = row_window(100, 2_000.0, 100.0, ROW_HEIGHT);
        let large = row_window(100, 2_000.0, 1_000.0, ROW_HEIGHT);

        assert_eq!(
            (
                small.visible_start,
                small.visible_end,
                small.start,
                small.end
            ),
            (20, 21, 14, 27)
        );
        assert_eq!(
            (
                large.visible_start,
                large.visible_end,
                large.start,
                large.end
            ),
            (20, 30, 2, 48)
        );
    }

    #[test]
    fn overscan_policy_clamps_to_documented_boundaries() {
        assert_eq!(overscan_rows(100.0, ROW_HEIGHT), MIN_OVERSCAN_ROWS);
        assert_eq!(overscan_rows(300.0, ROW_HEIGHT), MIN_OVERSCAN_ROWS);
        assert_eq!(overscan_rows(900.0, ROW_HEIGHT), MAX_OVERSCAN_ROWS);
        assert_eq!(overscan_rows(2_000.0, ROW_HEIGHT), MAX_OVERSCAN_ROWS);
        assert_eq!(overscan_rows(f32::NAN, ROW_HEIGHT), MIN_OVERSCAN_ROWS);
    }

    #[test]
    fn fewer_rows_than_viewport_renders_every_row() {
        let window = row_window(4, 0.0, 1_000.0, ROW_HEIGHT);

        assert_eq!((window.visible_start, window.visible_end), (0, 4));
        assert_eq!((window.start, window.end), (0, 4));
        assert_eq!((window.top_spacer, window.bottom_spacer), (0.0, 0.0));
    }

    #[test]
    fn exact_row_boundary_starts_at_the_next_row_without_overlap() {
        let window = row_window(100, ROW_HEIGHT, 300.0, ROW_HEIGHT);

        assert_eq!((window.visible_start, window.visible_end), (1, 4));
    }

    #[test]
    fn offset_beyond_content_keeps_a_trailing_overscan_window() {
        let window = row_window(10, 5_000.0, 300.0, ROW_HEIGHT);

        assert_eq!((window.visible_start, window.visible_end), (10, 10));
        assert_eq!((window.start, window.end), (4, 10));
    }

    #[test]
    fn for_cards_adds_copy_height_below_the_poster_aspect() {
        let cards = ArtworkGridMetrics::for_cards(416.0, 48.0);
        assert_eq!(cards.cell_width, 128.0);
        assert_eq!(cards.cell_height, 192.0 + 48.0);
        assert_eq!(cards.row_height, 240.0 + ROW_GAP);
    }

    #[test]
    fn width_measurement_changes_columns_and_cell_height() {
        let narrow = ArtworkGridMetrics::for_width(128.0);
        let wide = ArtworkGridMetrics::for_width(416.0);

        assert_eq!(narrow.columns, 1);
        assert_eq!(wide.columns, 3);
        assert_eq!(wide.cell_width, 128.0);
        assert_eq!(wide.cell_height, 192.0);
        assert_eq!(wide.row_height, 208.0);
    }

    #[test]
    fn from_scrollable_conversion_clips_the_viewport_before_the_grid() {
        assert_eq!(
            ArtworkGridViewport::from_scroll_geometry(80.0, 100.0, 120.0),
            ArtworkGridViewport {
                offset_y: 0.0,
                height: 60.0,
            }
        );
    }

    #[test]
    fn available_width_column_count_change_recomputes_window_math() {
        let item_count = 60_usize;
        let viewport = ArtworkGridViewport {
            offset_y: 1_000.0,
            height: 400.0,
        };
        let narrow_metrics = ArtworkGridMetrics::for_width(200.0);
        let wide_metrics = ArtworkGridMetrics::for_width(416.0);
        let narrow_row_count = item_count.div_ceil(narrow_metrics.columns);
        let wide_row_count = item_count.div_ceil(wide_metrics.columns);
        let narrow_window = row_window(
            narrow_row_count,
            viewport.offset_y,
            viewport.height,
            narrow_metrics.row_height,
        );
        let wide_window = row_window(
            wide_row_count,
            viewport.offset_y,
            viewport.height,
            wide_metrics.row_height,
        );

        assert_eq!(
            (
                narrow_metrics.columns,
                narrow_row_count,
                narrow_window.visible_start,
                narrow_window.visible_end,
                narrow_window.start,
                narrow_window.end,
                narrow_window.bottom_spacer,
            ),
            (1, 60, 3, 5, 0, 11, 15_468.0)
        );
        assert_eq!(
            (
                wide_metrics.columns,
                wide_row_count,
                wide_window.visible_start,
                wide_window.visible_end,
                wide_window.start,
                wide_window.end,
                wide_window.bottom_spacer,
            ),
            (3, 20, 4, 7, 0, 13, 1_440.0)
        );
    }

    #[test]
    fn every_window_preserves_content_extent_without_a_trailing_gap() {
        let row_count = 100;
        let expected_extent = row_count as f32 * ROW_HEIGHT - ROW_GAP;

        for window in [
            row_window(row_count, 0.0, 300.0, ROW_HEIGHT),
            row_window(row_count, 2_000.0, 300.0, ROW_HEIGHT),
            row_window(row_count, 9_700.0, 300.0, ROW_HEIGHT),
        ] {
            let mut rendered_extent = window.end.saturating_sub(window.start) as f32 * ROW_HEIGHT;
            if window.end == row_count {
                rendered_extent -= ROW_GAP;
            }

            assert_eq!(
                window.top_spacer + rendered_extent + window.bottom_spacer,
                expected_extent
            );
        }
    }

    #[test]
    fn empty_grid_has_no_spacers_or_rows() {
        assert_eq!(
            row_window(0, 200.0, 300.0, ROW_HEIGHT),
            RowWindow {
                visible_start: 0,
                visible_end: 0,
                start: 0,
                end: 0,
                top_spacer: 0.0,
                bottom_spacer: 0.0,
            }
        );
    }

    #[test]
    fn artwork_grid_builds_cells_with_global_indexes_for_viewport_window() {
        let built_indexes = RefCell::new(Vec::new());
        let metrics = ArtworkGridMetrics {
            columns: 4,
            cell_width: 100.0,
            cell_height: 100.0,
            row_height: 100.0,
        };
        // With row_height=100.0 and viewport_height=100.0, overscan is MIN_OVERSCAN_ROWS (6).
        // An offset_y of 800.0 puts visible_start at row 8, so window start is row 2 (8 - 6 = 2).
        // For columns=4, row 2 starts at global item index 8.
        let viewport = ArtworkGridViewport {
            offset_y: 800.0,
            height: 100.0,
        };
        let _element: Element<'_, ()> = artwork_grid(100, metrics, viewport, |index| {
            built_indexes.borrow_mut().push(index);
            iced::widget::Space::new().into()
        });

        let indexes = built_indexes.into_inner();
        assert_eq!(indexes.first(), Some(&8));
        // Window end is visible_end (9) + overscan (6) = 15.
        // Row 14 ends at global item index 60 (exclusive).
        assert_eq!(indexes.last(), Some(&59));
        assert_eq!(indexes, (8..60).collect::<Vec<_>>());
    }

    #[test]
    fn artwork_grid_empty_item_count_builds_no_cells() {
        let built_indexes = RefCell::new(Vec::new());
        let metrics = ArtworkGridMetrics {
            columns: 4,
            cell_width: 100.0,
            cell_height: 100.0,
            row_height: 100.0,
        };
        let viewport = ArtworkGridViewport {
            offset_y: 0.0,
            height: 300.0,
        };
        let _element: Element<'_, ()> = artwork_grid(0, metrics, viewport, |index| {
            built_indexes.borrow_mut().push(index);
            iced::widget::Space::new().into()
        });

        assert!(built_indexes.into_inner().is_empty());
    }
}
