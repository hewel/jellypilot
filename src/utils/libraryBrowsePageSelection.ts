/**
 * Pure display-to-page math for the library browse virtual grid.
 * Framework-independent: no Solid, TanStack, Effect, or Tauri dependencies.
 */

interface LibraryBrowsePageLocationOptions {
  displayIndex: number;
  totalRecordCount: number;
  pageSize: number;
  reverse: boolean;
}

interface LibraryBrowsePageLocation {
  pageStart: number;
  indexWithinPage: number;
}

interface LibraryBrowsePageStartsOptions {
  rowIndexes: readonly number[];
  columnCount: number;
  totalRecordCount: number;
  pageSize: number;
  reverse: boolean;
}

export function libraryBrowsePageLocationForDisplayIndex(
  options: LibraryBrowsePageLocationOptions,
): LibraryBrowsePageLocation | null {
  const { displayIndex, totalRecordCount, pageSize, reverse } = options;
  if (
    !Number.isInteger(displayIndex) ||
    !Number.isInteger(totalRecordCount) ||
    totalRecordCount < 0 ||
    !Number.isInteger(pageSize) ||
    pageSize <= 0
  ) {
    return null;
  }

  const serverIndex = reverse ? totalRecordCount - 1 - displayIndex : displayIndex;
  if (serverIndex < 0 || serverIndex >= totalRecordCount) {
    return null;
  }

  const pageStart = Math.floor(serverIndex / pageSize) * pageSize;
  return { pageStart, indexWithinPage: serverIndex - pageStart };
}

export function libraryBrowsePageStartsForRows(
  options: LibraryBrowsePageStartsOptions,
): readonly number[] {
  const { rowIndexes, columnCount, totalRecordCount, pageSize, reverse } = options;
  if (
    !Number.isInteger(columnCount) ||
    columnCount <= 0 ||
    !Number.isInteger(totalRecordCount) ||
    totalRecordCount < 0 ||
    !Number.isInteger(pageSize) ||
    pageSize <= 0
  ) {
    return [];
  }

  const required: number[] = [];
  const seen = new Set<number>();
  for (const rowIndex of rowIndexes) {
    if (!Number.isInteger(rowIndex) || rowIndex < 0) {
      continue;
    }

    for (let columnIndex = 0; columnIndex < columnCount; columnIndex += 1) {
      const location = libraryBrowsePageLocationForDisplayIndex({
        displayIndex: rowIndex * columnCount + columnIndex,
        totalRecordCount,
        pageSize,
        reverse,
      });
      if (location && !seen.has(location.pageStart)) {
        seen.add(location.pageStart);
        required.push(location.pageStart);
      }
    }
  }

  if (required.length > 0) {
    const lastValidStart = Math.floor((totalRecordCount - 1) / pageSize) * pageSize;
    const lookAheadStart = reverse
      ? Math.min(...required) - pageSize
      : Math.max(...required) + pageSize;
    if (lookAheadStart >= 0 && lookAheadStart <= lastValidStart && !seen.has(lookAheadStart)) {
      required.push(lookAheadStart);
    }
  }

  return required;
}

export function retainLibraryBrowsePages<T>(
  pages: Map<number, T>,
  retainedPageStarts: ReadonlySet<number>,
): Map<number, T> {
  let retainedPages: Map<number, T> | null = null;
  for (const pageStart of pages.keys()) {
    if (!retainedPageStarts.has(pageStart)) {
      retainedPages ??= new Map(pages);
      retainedPages.delete(pageStart);
    }
  }
  return retainedPages ?? pages;
}
