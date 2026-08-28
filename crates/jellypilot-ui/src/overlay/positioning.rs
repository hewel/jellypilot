//! Geometry for anchor-relative overlay placement.

use iced::{Point, Rectangle, Size};

/// Preferred side of an anchor for a floating layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    Above,
    Below,
    Start,
    End,
}

impl Placement {
    const fn opposite(self) -> Self {
        match self {
            Self::Above => Self::Below,
            Self::Below => Self::Above,
            Self::Start => Self::End,
            Self::End => Self::Start,
        }
    }
}

/// Alignment of a floating layer along the anchor's cross axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Start,
    Center,
    End,
}
/// Controls anchor-relative placement, flipping, and viewport clamping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositioningOptions {
    pub preferred: Placement,
    pub alignment: Alignment,
    pub gap: f32,
    pub clamp_to_viewport: bool,
    pub flip_when_overflow: bool,
}

/// The resolved position and side of a floating layer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerPosition {
    pub point: Point,
    pub placement: Placement,
}

/// Positions a floating layer relative to an anchor.
///
/// If the preferred side overflows the viewport and its opposite side has less
/// overflow, the opposite side is used. The final point can then be clamped so
/// the layer remains within the viewport.
#[must_use]
pub fn position_layer(
    anchor_bounds: Rectangle,
    layer_size: Size,
    viewport_bounds: Rectangle,
    options: PositioningOptions,
) -> LayerPosition {
    let PositioningOptions {
        preferred,
        alignment,
        gap,
        clamp_to_viewport,
        flip_when_overflow,
    } = options;
    let preferred_point = unclamped_position(anchor_bounds, layer_size, preferred, alignment, gap);
    let opposite = preferred.opposite();
    let opposite_point = unclamped_position(anchor_bounds, layer_size, opposite, alignment, gap);

    let preferred_overflow =
        main_axis_overflow(preferred_point, layer_size, viewport_bounds, preferred);
    let opposite_overflow =
        main_axis_overflow(opposite_point, layer_size, viewport_bounds, opposite);
    let (point, placement) =
        if flip_when_overflow && preferred_overflow > 0.0 && opposite_overflow < preferred_overflow
        {
            (opposite_point, opposite)
        } else {
            (preferred_point, preferred)
        };

    LayerPosition {
        point: if clamp_to_viewport {
            Point::new(
                clamp_axis(
                    point.x,
                    layer_size.width,
                    viewport_bounds.x,
                    viewport_bounds.width,
                ),
                clamp_axis(
                    point.y,
                    layer_size.height,
                    viewport_bounds.y,
                    viewport_bounds.height,
                ),
            )
        } else {
            point
        },
        placement,
    }
}

fn unclamped_position(
    anchor: Rectangle,
    layer: Size,
    placement: Placement,
    alignment: Alignment,
    gap: f32,
) -> Point {
    match placement {
        Placement::Above => Point::new(
            aligned_start(anchor.x, anchor.width, layer.width, alignment),
            anchor.y - layer.height - gap,
        ),
        Placement::Below => Point::new(
            aligned_start(anchor.x, anchor.width, layer.width, alignment),
            anchor.y + anchor.height + gap,
        ),
        Placement::Start => Point::new(
            anchor.x - layer.width - gap,
            aligned_start(anchor.y, anchor.height, layer.height, alignment),
        ),
        Placement::End => Point::new(
            anchor.x + anchor.width + gap,
            aligned_start(anchor.y, anchor.height, layer.height, alignment),
        ),
    }
}

fn aligned_start(
    anchor_start: f32,
    anchor_size: f32,
    layer_size: f32,
    alignment: Alignment,
) -> f32 {
    match alignment {
        Alignment::Start => anchor_start,
        Alignment::Center => anchor_start + (anchor_size - layer_size) / 2.0,
        Alignment::End => anchor_start + anchor_size - layer_size,
    }
}

fn main_axis_overflow(point: Point, layer: Size, viewport: Rectangle, placement: Placement) -> f32 {
    let (start, size, viewport_start, viewport_size) = match placement {
        Placement::Above | Placement::Below => (point.y, layer.height, viewport.y, viewport.height),
        Placement::Start | Placement::End => (point.x, layer.width, viewport.x, viewport.width),
    };
    let viewport_end = viewport_start + viewport_size;

    (viewport_start - start).max(0.0) + (start + size - viewport_end).max(0.0)
}

fn clamp_axis(position: f32, layer_size: f32, viewport_start: f32, viewport_size: f32) -> f32 {
    let max_position = viewport_start + viewport_size - layer_size;

    if max_position < viewport_start {
        viewport_start
    } else {
        position.clamp(viewport_start, max_position)
    }
}

#[cfg(test)]
mod tests {
    use iced::{Point, Rectangle, Size};

    use super::{position_layer, Alignment, LayerPosition, Placement, PositioningOptions};

    const VIEWPORT: Rectangle = Rectangle {
        x: 10.0,
        y: 20.0,
        width: 300.0,
        height: 200.0,
    };
    const LAYER: Size = Size::new(80.0, 60.0);

    fn position(anchor: Rectangle, preferred: Placement, alignment: Alignment) -> LayerPosition {
        position_layer(
            anchor,
            LAYER,
            VIEWPORT,
            PositioningOptions {
                preferred,
                alignment,
                gap: 8.0,
                clamp_to_viewport: true,
                flip_when_overflow: true,
            },
        )
    }

    #[test]
    fn preferred_side_is_kept_when_it_fits() {
        assert_eq!(
            position(
                Rectangle::new(Point::new(100.0, 80.0), Size::new(40.0, 30.0)),
                Placement::Below,
                Alignment::Start,
            ),
            LayerPosition {
                point: Point::new(100.0, 118.0),
                placement: Placement::Below,
            }
        );
    }

    #[test]
    fn below_flips_above_at_bottom_edge() {
        assert_eq!(
            position(
                Rectangle::new(Point::new(100.0, 190.0), Size::new(40.0, 20.0)),
                Placement::Below,
                Alignment::Start,
            ),
            LayerPosition {
                point: Point::new(100.0, 122.0),
                placement: Placement::Above,
            }
        );
    }

    #[test]
    fn above_flips_below_at_top_edge() {
        assert_eq!(
            position(
                Rectangle::new(Point::new(100.0, 30.0), Size::new(40.0, 20.0)),
                Placement::Above,
                Alignment::Start,
            ),
            LayerPosition {
                point: Point::new(100.0, 58.0),
                placement: Placement::Below,
            }
        );
    }

    #[test]
    fn start_flips_end_at_left_edge() {
        assert_eq!(
            position(
                Rectangle::new(Point::new(20.0, 80.0), Size::new(30.0, 30.0)),
                Placement::Start,
                Alignment::Start,
            ),
            LayerPosition {
                point: Point::new(58.0, 80.0),
                placement: Placement::End,
            }
        );
    }

    #[test]
    fn end_flips_start_at_right_edge() {
        assert_eq!(
            position(
                Rectangle::new(Point::new(280.0, 80.0), Size::new(20.0, 30.0)),
                Placement::End,
                Alignment::Start,
            ),
            LayerPosition {
                point: Point::new(192.0, 80.0),
                placement: Placement::Start,
            }
        );
    }

    #[test]
    fn cross_axis_start_is_clamped_to_viewport_start() {
        assert_eq!(
            position(
                Rectangle::new(Point::new(-20.0, 80.0), Size::new(30.0, 20.0)),
                Placement::Below,
                Alignment::Start,
            )
            .point,
            Point::new(10.0, 108.0)
        );
    }

    #[test]
    fn cross_axis_end_is_clamped_to_viewport_end() {
        assert_eq!(
            position(
                Rectangle::new(Point::new(290.0, 80.0), Size::new(40.0, 20.0)),
                Placement::Below,
                Alignment::End,
            )
            .point,
            Point::new(230.0, 108.0)
        );
    }

    #[test]
    fn center_alignment_is_applied_before_clamping() {
        assert_eq!(
            position(
                Rectangle::new(Point::new(120.0, 80.0), Size::new(20.0, 20.0)),
                Placement::Below,
                Alignment::Center,
            )
            .point,
            Point::new(90.0, 108.0)
        );
    }

    #[test]
    fn oversized_layer_clamps_to_viewport_origin() {
        let result = position_layer(
            Rectangle::new(Point::new(100.0, 100.0), Size::new(20.0, 20.0)),
            Size::new(400.0, 300.0),
            VIEWPORT,
            PositioningOptions {
                preferred: Placement::Below,
                alignment: Alignment::Center,
                gap: 8.0,
                clamp_to_viewport: true,
                flip_when_overflow: true,
            },
        );

        assert_eq!(result.point, VIEWPORT.position());
    }

    #[test]
    fn flip_is_skipped_when_both_sides_overflow_equally() {
        let result = position_layer(
            Rectangle::new(Point::new(100.0, 110.0), Size::new(20.0, 20.0)),
            Size::new(80.0, 220.0),
            VIEWPORT,
            PositioningOptions {
                preferred: Placement::Below,
                alignment: Alignment::Start,
                gap: 0.0,
                clamp_to_viewport: true,
                flip_when_overflow: true,
            },
        );

        assert_eq!(result.placement, Placement::Below);
    }

    #[test]
    fn less_overflowing_side_wins_when_neither_side_fits() {
        let result = position_layer(
            Rectangle::new(Point::new(100.0, 160.0), Size::new(20.0, 20.0)),
            Size::new(80.0, 150.0),
            VIEWPORT,
            PositioningOptions {
                preferred: Placement::Below,
                alignment: Alignment::Start,
                gap: 8.0,
                clamp_to_viewport: true,
                flip_when_overflow: true,
            },
        );

        assert_eq!(result.placement, Placement::Above);
    }

    #[test]
    fn disabling_flip_keeps_preferred_side_then_clamps() {
        let result = position_layer(
            Rectangle::new(Point::new(100.0, 190.0), Size::new(20.0, 20.0)),
            LAYER,
            VIEWPORT,
            PositioningOptions {
                preferred: Placement::Below,
                alignment: Alignment::Start,
                gap: 8.0,
                clamp_to_viewport: true,
                flip_when_overflow: false,
            },
        );

        assert_eq!(
            result,
            LayerPosition {
                point: Point::new(100.0, 160.0),
                placement: Placement::Below,
            }
        );
    }

    #[test]
    fn disabling_clamp_preserves_the_unclamped_flipped_point() {
        let result = position_layer(
            Rectangle::new(Point::new(-30.0, 190.0), Size::new(20.0, 20.0)),
            LAYER,
            VIEWPORT,
            PositioningOptions {
                preferred: Placement::Below,
                alignment: Alignment::Start,
                gap: 8.0,
                clamp_to_viewport: false,
                flip_when_overflow: true,
            },
        );

        assert_eq!(
            result,
            LayerPosition {
                point: Point::new(-30.0, 122.0),
                placement: Placement::Above,
            }
        );
    }

    #[test]
    fn exact_fit_on_preferred_side_does_not_flip() {
        assert_eq!(
            position(
                Rectangle::new(Point::new(100.0, 132.0), Size::new(40.0, 20.0)),
                Placement::Below,
                Alignment::Start,
            ),
            LayerPosition {
                point: Point::new(100.0, 160.0),
                placement: Placement::Below,
            }
        );
    }

    #[test]
    fn zero_size_anchor_positions_from_its_point() {
        assert_eq!(
            position(
                Rectangle::new(Point::new(100.0, 80.0), Size::ZERO),
                Placement::Below,
                Alignment::Start,
            ),
            LayerPosition {
                point: Point::new(100.0, 88.0),
                placement: Placement::Below,
            }
        );
    }

    #[test]
    fn zero_size_panel_preserves_preferred_placement() {
        let result = position_layer(
            Rectangle::new(Point::new(100.0, 80.0), Size::new(40.0, 30.0)),
            Size::ZERO,
            VIEWPORT,
            PositioningOptions {
                preferred: Placement::Below,
                alignment: Alignment::Start,
                gap: 8.0,
                clamp_to_viewport: true,
                flip_when_overflow: true,
            },
        );

        assert_eq!(
            result,
            LayerPosition {
                point: Point::new(100.0, 118.0),
                placement: Placement::Below,
            }
        );
    }
}
