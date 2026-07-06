import { expect, test } from '@rstest/core';

import {
  LIBRARY_BROWSE_GRID_TEMPLATE_COLUMNS,
  libraryBrowseColumnCount,
} from '../src/utils/libraryBrowseLayout';

test('library browse column count auto-fits by available width', () => {
  expect(libraryBrowseColumnCount(360)).toBe(2);
  expect(libraryBrowseColumnCount(800)).toBe(4);
  expect(libraryBrowseColumnCount(1024)).toBe(6);
});

test('library browse column count falls back to one column for unknown widths', () => {
  expect(libraryBrowseColumnCount(0)).toBe(1);
  expect(libraryBrowseColumnCount(Number.NaN)).toBe(1);
});

test('library browse grid preserves empty tracks so the last row does not stretch', () => {
  expect(LIBRARY_BROWSE_GRID_TEMPLATE_COLUMNS).toContain('repeat(auto-fill');
  expect(LIBRARY_BROWSE_GRID_TEMPLATE_COLUMNS).not.toContain('repeat(auto-fit');
});
