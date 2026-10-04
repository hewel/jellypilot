//! Validated, display-free lookup of server-generated seek thumbnail tiles.

/// Grid geometry of one Trickplay rendition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasLayout {
    width: u32,
    height: u32,
    columns: u32,
    rows: u32,
    thumbnail_count: u32,
    interval_ms: u32,
}

/// One frame's source rectangle in its shared atlas image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreviewTile {
    pub atlas_index: u32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl AtlasLayout {
    /// Rejects empty, overflowing, or excessively large server-provided grids.
    /// An accepted atlas has at most 16 megapixels and 8192 pixels per side.
    pub fn new(
        width: u32,
        height: u32,
        columns: u32,
        rows: u32,
        thumbnail_count: u32,
        interval_ms: u32,
    ) -> Option<Self> {
        if [width, height, columns, rows, thumbnail_count, interval_ms].contains(&0) {
            return None;
        }
        let atlas_width = width.checked_mul(columns)?;
        let atlas_height = height.checked_mul(rows)?;
        if atlas_width > 8192
            || atlas_height > 8192
            || atlas_width.checked_mul(atlas_height)? > 16 * 1024 * 1024
        {
            return None;
        }
        Some(Self {
            width,
            height,
            columns,
            rows,
            thumbnail_count,
            interval_ms,
        })
    }

    /// Looks up the frame at or before the target time. End-of-media positions
    /// select the final real thumbnail, never padding in the final atlas.
    pub fn tile_at(self, seconds: f64) -> Option<PreviewTile> {
        if !seconds.is_finite() || seconds < 0.0 {
            return None;
        }
        let frame = ((seconds * 1000.0 / f64::from(self.interval_ms)).floor() as u32)
            .min(self.thumbnail_count - 1);
        let per_atlas = self.columns * self.rows;
        let offset = frame % per_atlas;
        Some(PreviewTile {
            atlas_index: frame / per_atlas,
            x: (offset % self.columns) * self.width,
            y: (offset / self.columns) * self.height,
            width: self.width,
            height: self.height,
        })
    }

    /// Exact dimensions required for the decoded atlas; padding remains part
    /// of the final atlas so all frame coordinates share the same grid.
    pub fn atlas_size(self) -> (u32, u32) {
        (self.width * self.columns, self.height * self.rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_boundaries_cross_rows_and_atlases_without_selecting_padding() {
        let grid = AtlasLayout::new(160, 90, 3, 2, 9, 2000).unwrap();
        let samples: Vec<_> = [0.0, 5.999, 6.0, 11.999, 12.0, 18.0, f64::MAX]
            .into_iter()
            .map(|time| {
                let tile = grid.tile_at(time).unwrap();
                (tile.atlas_index, tile.x, tile.y)
            })
            .collect();
        assert_eq!(
            samples,
            [
                (0, 0, 0),
                (0, 320, 0),
                (0, 0, 90),
                (0, 320, 90),
                (1, 0, 0),
                (1, 320, 0),
                (1, 320, 0)
            ]
        );
    }

    #[test]
    fn untrusted_geometry_cannot_overflow_or_exceed_decode_budget() {
        for fields in [
            [0, 90, 10, 10, 100, 1000],
            [160, 90, 0, 10, 100, 1000],
            [160, 90, 10, 10, 0, 1000],
            [160, 90, 10, 10, 100, 0],
            [u32::MAX, 90, 10, 10, 100, 1000],
            [512, 512, 16, 16, 100, 1000],
            [1, 1, 8193, 1, 100, 1000],
        ] {
            let [width, height, columns, rows, count, interval] = fields;
            assert!(AtlasLayout::new(width, height, columns, rows, count, interval).is_none());
        }
    }

    #[test]
    fn invalid_target_does_not_select_a_frame() {
        let grid = AtlasLayout::new(160, 90, 10, 10, 200, 1000).unwrap();
        for target in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
            assert_eq!(grid.tile_at(target), None);
        }
    }
}
