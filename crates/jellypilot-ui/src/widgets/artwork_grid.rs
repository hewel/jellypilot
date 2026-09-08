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

/// The full parent scroll viewport in signed grid-local coordinates.
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
        let offset_y = viewport_offset - grid_scroll_margin;
        Self {
            offset_y: if offset_y.is_finite() { offset_y } else { 0.0 },
            height: if offset_y.is_finite() {
                finite_non_negative(viewport_height)
            } else {
                0.0
            },
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
/// The parent supplies measured, grid-local viewport geometry and grid width.
/// Before its first measurement it may supply a bounded fallback viewport.
/// Cells look up items by global item index and may contain any normal
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
    if metrics.columns == 0 {
        return Space::new().into();
    }
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
    if row_count == 0 || !row_height.is_finite() || row_height <= 0.0 {
        return RowWindow {
            visible_start: 0,
            visible_end: 0,
            start: 0,
            end: 0,
            top_spacer: 0.0,
            bottom_spacer: 0.0,
        };
    }

    let viewport_height = if offset_y.is_finite() {
        finite_non_negative(viewport_height)
    } else {
        0.0
    };
    let visible_start = ((offset_y / row_height).floor() as usize).min(row_count);
    let visible_end = if viewport_height == 0.0 {
        visible_start
    } else {
        (((offset_y + viewport_height) / row_height).ceil() as usize).min(row_count)
    };
    let (start, end) = if viewport_height == 0.0 {
        (0, 0)
    } else {
        (
            (((offset_y - viewport_height).max(0.0) / row_height).floor() as usize).min(row_count),
            (((offset_y + 2.0 * viewport_height) / row_height).ceil() as usize).min(row_count),
        )
    };

    let remaining_rows = row_count.saturating_sub(end);

    RowWindow {
        visible_start,
        visible_end,
        start,
        end,
        top_spacer: if start == row_count {
            (start as f32 * row_height - ROW_GAP).max(0.0)
        } else {
            start as f32 * row_height
        },
        bottom_spacer: if remaining_rows == 0 {
            0.0
        } else {
            (remaining_rows as f32 * row_height - ROW_GAP).max(0.0)
        },
    }
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
        artwork_grid, row_window, ArtworkGridMetrics, ArtworkGridViewport, RowWindow, ROW_GAP,
    };

    const ROW_HEIGHT: f32 = 100.0;

    #[test]
    fn prefetch_window_rounds_outward_without_expanding_visible_classification() {
        let offset = 200_025.0;
        let height = 325.0;
        let window = row_window(100_000, offset, height, ROW_HEIGHT);
        assert_eq!((window.visible_start, window.visible_end), (2_000, 2_004));
        let start_y = window.start as f32 * ROW_HEIGHT;
        let end_y = window.end as f32 * ROW_HEIGHT;
        assert!(start_y <= offset - height && start_y + ROW_HEIGHT > offset - height);
        assert!(end_y >= offset + 2.0 * height && end_y - ROW_HEIGHT < offset + 2.0 * height);
        assert!(window.end - window.start <= (3.0 * height / ROW_HEIGHT).ceil() as usize + 1);
    }

    #[test]
    fn prefetch_window_clips_at_both_content_edges() {
        let top = row_window(100, 0.0, 300.0, ROW_HEIGHT);
        let bottom = row_window(100, 9_700.0, 300.0, ROW_HEIGHT);
        assert_eq!((top.start, top.end), (0, 6));
        assert_eq!((bottom.start, bottom.end), (94, 100));
        assert_eq!(top.top_spacer, 0.0);
        assert_eq!(bottom.bottom_spacer, 0.0);
    }

    #[test]
    fn viewport_resize_recomputes_visible_and_prefetch_rows() {
        let small = row_window(100, 2_000.0, 100.0, ROW_HEIGHT);
        let large = row_window(100, 2_000.0, 1_000.0, ROW_HEIGHT);
        assert_eq!(small.visible_start, large.visible_start);
        assert_eq!(large.visible_end - small.visible_end, 9);
        assert_eq!(small.start - large.start, 9);
        assert_eq!(large.end - small.end, 18);
    }

    #[test]
    fn unusable_geometry_materializes_no_rows() {
        for invalid in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let invalid_viewport = row_window(100, 250.0, invalid, ROW_HEIGHT);
            let invalid_row = row_window(100, 250.0, 300.0, invalid);
            assert_eq!(invalid_viewport.start, invalid_viewport.end);
            assert_eq!(invalid_viewport.visible_start, invalid_viewport.visible_end);
            assert_eq!(invalid_row.start, invalid_row.end);
        }
        for invalid_offset in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let window = row_window(100, invalid_offset, 300.0, ROW_HEIGHT);
            assert_eq!(window.start, window.end);
            assert_eq!(window.visible_start, window.visible_end);
        }
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
    fn offset_beyond_prefetch_distance_materializes_no_rows() {
        let window = row_window(10, 5_000.0, 300.0, ROW_HEIGHT);

        assert_eq!((window.visible_start, window.visible_end), (10, 10));
        assert_eq!((window.start, window.end), (10, 10));
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
    fn from_scrollable_conversion_preserves_signed_top_and_full_height() {
        assert_eq!(
            ArtworkGridViewport::from_scroll_geometry(80.0, 100.0, 120.0),
            ArtworkGridViewport {
                offset_y: -40.0,
                height: 100.0,
            }
        );
    }

    #[test]
    fn invalid_scroll_geometry_cannot_materialize_demand_rows() {
        for (offset, height, margin) in [
            (f32::NAN, 300.0, 120.0),
            (0.0, f32::INFINITY, 120.0),
            (0.0, 300.0, f32::NAN),
            (0.0, 0.0, 120.0),
        ] {
            let viewport = ArtworkGridViewport::from_scroll_geometry(offset, height, margin);
            let window = row_window(100, viewport.offset_y, viewport.height, ROW_HEIGHT);
            assert_eq!(window.start, window.end);
        }
    }

    #[test]
    fn header_offset_keeps_the_preceding_demanded_row_when_scrolling_both_ways() {
        for raw_offset in [520.0, 920.0, 520.0] {
            let viewport = ArtworkGridViewport::from_scroll_geometry(raw_offset, 300.0, 120.0);
            let window = row_window(100, viewport.offset_y, viewport.height, ROW_HEIGHT);
            assert_eq!(
                window.start,
                ((raw_offset - 120.0 - 300.0) / ROW_HEIGHT) as usize
            );
            assert_eq!(
                window.end,
                ((raw_offset - 120.0 + 600.0) / ROW_HEIGHT) as usize
            );
        }
        let above_grid = row_window(100, -120.0, 300.0, ROW_HEIGHT);
        assert_eq!((above_grid.visible_start, above_grid.visible_end), (0, 2));
        assert_eq!((above_grid.start, above_grid.end), (0, 5));
        let distant = row_window(100, -700.0, 300.0, ROW_HEIGHT);
        assert_eq!((distant.start, distant.end), (0, 0));
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

        assert_ne!(narrow_metrics.columns, wide_metrics.columns);
        for (metrics, window) in [(narrow_metrics, narrow_window), (wide_metrics, wide_window)] {
            let first_y = window.start as f32 * metrics.row_height;
            let end_y = window.end as f32 * metrics.row_height;
            assert!(first_y <= viewport.offset_y - viewport.height);
            assert!(first_y + metrics.row_height > viewport.offset_y - viewport.height);
            assert!(end_y >= viewport.offset_y + 2.0 * viewport.height);
            assert!(end_y - metrics.row_height < viewport.offset_y + 2.0 * viewport.height);
        }
    }

    #[test]
    fn every_window_preserves_content_extent_without_a_trailing_gap() {
        let row_count = 100;
        let expected_extent = row_count as f32 * ROW_HEIGHT - ROW_GAP;

        for window in [
            row_window(row_count, 0.0, 300.0, ROW_HEIGHT),
            row_window(row_count, 2_000.0, 300.0, ROW_HEIGHT),
            row_window(row_count, 9_700.0, 300.0, ROW_HEIGHT),
            row_window(row_count, 20_000.0, 300.0, ROW_HEIGHT),
            row_window(row_count, 250.0, 0.0, ROW_HEIGHT),
        ] {
            let mut rendered_extent = window.end.saturating_sub(window.start) as f32 * ROW_HEIGHT;
            if window.end == row_count && window.start < window.end {
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
        let viewport = ArtworkGridViewport {
            offset_y: 200_025.0,
            height: 2_025.0,
        };
        let _element: Element<'_, ()> = artwork_grid(1_000_000, metrics, viewport, |index| {
            built_indexes.borrow_mut().push(index);
            iced::widget::Space::new().into()
        });

        let indexes = built_indexes.into_inner();
        let expected_rows = 1_980..2_041;
        assert_eq!(
            indexes,
            (expected_rows.start * metrics.columns..expected_rows.end * metrics.columns)
                .collect::<Vec<_>>()
        );
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
