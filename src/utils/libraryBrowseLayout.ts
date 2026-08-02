export const LIBRARY_BROWSE_MIN_CARD_WIDTH_PX = 160;
// Mirrors browseRoute.styles.ts grid: columnGap '3' (12px), rowGap '4' (16px).
export const LIBRARY_BROWSE_GRID_COLUMN_GAP_PX = 12;
export const LIBRARY_BROWSE_GRID_ROW_GAP_PX = 16;
export const LIBRARY_BROWSE_GRID_TEMPLATE_COLUMNS =
  'repeat(auto-fill, minmax(min(100%, 160px), 1fr))';
const LIBRARY_BROWSE_MIN_OVERSCAN_ROWS = 6;
const LIBRARY_BROWSE_MAX_OVERSCAN_ROWS = 18;

export function libraryBrowseColumnCount(width: number): number {
  if (!Number.isFinite(width) || width <= 0) {
    return 1;
  }

  return Math.max(
    1,
    Math.floor(
      (width + LIBRARY_BROWSE_GRID_COLUMN_GAP_PX) /
        (LIBRARY_BROWSE_MIN_CARD_WIDTH_PX + LIBRARY_BROWSE_GRID_COLUMN_GAP_PX),
    ),
  );
}

export const LIBRARY_BROWSE_CARD_ASPECT_RATIO = 1.5;
// Browse cards use the shared below-artwork metadata block: 12px top padding,
// 24px title, 2px gap, and 20px subtitle.
// Mirrors VideoCard.styles.ts; update when the card styles change.
export const LIBRARY_BROWSE_CARD_BODY_HEIGHT_PX = 58;

export function libraryBrowseVirtualRowHeight(width: number): number {
  const columns = libraryBrowseColumnCount(width);
  const usableWidth =
    Number.isFinite(width) && width > 0 ? width : LIBRARY_BROWSE_MIN_CARD_WIDTH_PX;
  const cardWidth = (usableWidth - LIBRARY_BROWSE_GRID_COLUMN_GAP_PX * (columns - 1)) / columns;
  return Math.ceil(
    cardWidth * LIBRARY_BROWSE_CARD_ASPECT_RATIO +
      LIBRARY_BROWSE_CARD_BODY_HEIGHT_PX +
      LIBRARY_BROWSE_GRID_ROW_GAP_PX,
  );
}

export function libraryBrowseVirtualOverscanRows(
  viewportHeight: number,
  rowHeight: number,
): number {
  if (
    !Number.isFinite(viewportHeight) ||
    viewportHeight <= 0 ||
    !Number.isFinite(rowHeight) ||
    rowHeight <= 0
  ) {
    return LIBRARY_BROWSE_MIN_OVERSCAN_ROWS;
  }

  const visibleRows = Math.ceil(viewportHeight / rowHeight);
  return Math.min(
    LIBRARY_BROWSE_MAX_OVERSCAN_ROWS,
    Math.max(LIBRARY_BROWSE_MIN_OVERSCAN_ROWS, visibleRows * 2),
  );
}
