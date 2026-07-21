export const LIBRARY_BROWSE_MIN_CARD_WIDTH_PX = 160;
export const LIBRARY_BROWSE_GRID_GAP_PX = 12;
export const LIBRARY_BROWSE_GRID_TEMPLATE_COLUMNS =
  'repeat(auto-fill, minmax(min(100%, 160px), 1fr))';

export function libraryBrowseColumnCount(width: number): number {
  if (!Number.isFinite(width) || width <= 0) {
    return 1;
  }

  return Math.max(
    1,
    Math.floor(
      (width + LIBRARY_BROWSE_GRID_GAP_PX) /
        (LIBRARY_BROWSE_MIN_CARD_WIDTH_PX + LIBRARY_BROWSE_GRID_GAP_PX),
    ),
  );
}

export const LIBRARY_BROWSE_CARD_ASPECT_RATIO = 1.5;
// Card chrome around the artwork, in px: 1px card top border + 64px body
// (8px pt + 24px title + 4px gap + 16px subtitle + 12px pb) + 1px card bottom border.
// Mirrors VideoCard.styles.ts body/aspect values; update when the card styles change.
export const LIBRARY_BROWSE_CARD_CHROME_HEIGHT_PX = 66;
// Card left + right 1px borders shrink the artwork below the grid track width.
const LIBRARY_BROWSE_CARD_SIDE_BORDERS_PX = 2;

export function libraryBrowseVirtualRowHeight(width: number): number {
  const columns = libraryBrowseColumnCount(width);
  const usableWidth =
    Number.isFinite(width) && width > 0 ? width : LIBRARY_BROWSE_MIN_CARD_WIDTH_PX;
  const cardWidth = (usableWidth - LIBRARY_BROWSE_GRID_GAP_PX * (columns - 1)) / columns;
  return Math.ceil(
    (cardWidth - LIBRARY_BROWSE_CARD_SIDE_BORDERS_PX) * LIBRARY_BROWSE_CARD_ASPECT_RATIO +
      LIBRARY_BROWSE_CARD_CHROME_HEIGHT_PX +
      LIBRARY_BROWSE_GRID_GAP_PX,
  );
}
