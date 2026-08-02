import { expect, test } from '@rstest/core';

import {
  libraryBrowsePageLocationForDisplayIndex,
  libraryBrowsePageStartsForRows,
  retainLibraryBrowsePages,
} from '../src/utils/libraryBrowsePageSelection';

test('library browse page location maps display indexes directly in normal order', () => {
  expect(
    libraryBrowsePageLocationForDisplayIndex({
      displayIndex: 0,
      totalRecordCount: 125,
      pageSize: 24,
      reverse: false,
    }),
  ).toEqual({ pageStart: 0, indexWithinPage: 0 });
  expect(
    libraryBrowsePageLocationForDisplayIndex({
      displayIndex: 30,
      totalRecordCount: 125,
      pageSize: 24,
      reverse: false,
    }),
  ).toEqual({ pageStart: 24, indexWithinPage: 6 });
});

test('library browse page location lands on both sides of a page boundary', () => {
  expect(
    libraryBrowsePageLocationForDisplayIndex({
      displayIndex: 23,
      totalRecordCount: 125,
      pageSize: 24,
      reverse: false,
    }),
  ).toEqual({ pageStart: 0, indexWithinPage: 23 });
  expect(
    libraryBrowsePageLocationForDisplayIndex({
      displayIndex: 24,
      totalRecordCount: 125,
      pageSize: 24,
      reverse: false,
    }),
  ).toEqual({ pageStart: 24, indexWithinPage: 0 });
});

test('library browse page location addresses the partial final page', () => {
  expect(
    libraryBrowsePageLocationForDisplayIndex({
      displayIndex: 124,
      totalRecordCount: 125,
      pageSize: 24,
      reverse: false,
    }),
  ).toEqual({ pageStart: 120, indexWithinPage: 4 });
});

test('library browse page location mirrors display indexes across the result set in reverse order', () => {
  expect(
    libraryBrowsePageLocationForDisplayIndex({
      displayIndex: 0,
      totalRecordCount: 125,
      pageSize: 24,
      reverse: true,
    }),
  ).toEqual({ pageStart: 120, indexWithinPage: 4 });
  expect(
    libraryBrowsePageLocationForDisplayIndex({
      displayIndex: 4,
      totalRecordCount: 125,
      pageSize: 24,
      reverse: true,
    }),
  ).toEqual({ pageStart: 120, indexWithinPage: 0 });
  expect(
    libraryBrowsePageLocationForDisplayIndex({
      displayIndex: 5,
      totalRecordCount: 125,
      pageSize: 24,
      reverse: true,
    }),
  ).toEqual({ pageStart: 96, indexWithinPage: 23 });
  expect(
    libraryBrowsePageLocationForDisplayIndex({
      displayIndex: 124,
      totalRecordCount: 125,
      pageSize: 24,
      reverse: true,
    }),
  ).toEqual({ pageStart: 0, indexWithinPage: 0 });
});

test('library browse page location rejects invalid and out-of-range display locations', () => {
  for (const displayIndex of [-1, 125, Number.NaN, Number.POSITIVE_INFINITY, 1.5]) {
    expect(
      libraryBrowsePageLocationForDisplayIndex({
        displayIndex,
        totalRecordCount: 125,
        pageSize: 24,
        reverse: false,
      }),
    ).toBeNull();
    expect(
      libraryBrowsePageLocationForDisplayIndex({
        displayIndex,
        totalRecordCount: 125,
        pageSize: 24,
        reverse: true,
      }),
    ).toBeNull();
  }
});

test('library browse page location rejects invalid planner geometry', () => {
  expect(
    libraryBrowsePageLocationForDisplayIndex({
      displayIndex: 0,
      totalRecordCount: 0,
      pageSize: 24,
      reverse: false,
    }),
  ).toBeNull();
  for (const totalRecordCount of [-1, 1.5, Number.NaN]) {
    expect(
      libraryBrowsePageLocationForDisplayIndex({
        displayIndex: 0,
        totalRecordCount,
        pageSize: 24,
        reverse: false,
      }),
    ).toBeNull();
  }
  for (const pageSize of [0, -24, 2.5, Number.NaN]) {
    expect(
      libraryBrowsePageLocationForDisplayIndex({
        displayIndex: 0,
        totalRecordCount: 125,
        pageSize,
        reverse: false,
      }),
    ).toBeNull();
  }
});

test('library browse page starts expand one row spanning two server pages', () => {
  expect(
    libraryBrowsePageStartsForRows({
      rowIndexes: [3],
      columnCount: 7,
      totalRecordCount: 125,
      pageSize: 24,
      reverse: false,
    }),
  ).toEqual([0, 24, 48]);
});

test('library browse page starts deduplicate pages in row encounter order', () => {
  expect(
    libraryBrowsePageStartsForRows({
      rowIndexes: [4, 3],
      columnCount: 7,
      totalRecordCount: 125,
      pageSize: 24,
      reverse: false,
    }),
  ).toEqual([24, 0, 48]);
});

test('library browse page starts append the normal look-ahead after every required page', () => {
  expect(
    libraryBrowsePageStartsForRows({
      rowIndexes: [0],
      columnCount: 4,
      totalRecordCount: 8,
      pageSize: 4,
      reverse: false,
    }),
  ).toEqual([0, 4]);
});

test('library browse page starts omit the normal look-ahead beyond the final page', () => {
  expect(
    libraryBrowsePageStartsForRows({
      rowIndexes: [17],
      columnCount: 7,
      totalRecordCount: 125,
      pageSize: 24,
      reverse: false,
    }),
  ).toEqual([96, 120]);
  expect(
    libraryBrowsePageStartsForRows({
      rowIndexes: [1],
      columnCount: 4,
      totalRecordCount: 8,
      pageSize: 4,
      reverse: false,
    }),
  ).toEqual([4]);
});

test('library browse page starts look ahead toward lower pages in reverse order', () => {
  expect(
    libraryBrowsePageStartsForRows({
      rowIndexes: [0],
      columnCount: 7,
      totalRecordCount: 125,
      pageSize: 24,
      reverse: true,
    }),
  ).toEqual([120, 96, 72]);
  expect(
    libraryBrowsePageStartsForRows({
      rowIndexes: [0, 1, 2, 3, 4, 5, 6, 7, 8],
      columnCount: 7,
      totalRecordCount: 125,
      pageSize: 24,
      reverse: true,
    }),
  ).toEqual([120, 96, 72, 48, 24]);
});

test('library browse page starts include the reverse look-ahead down to page zero', () => {
  expect(
    libraryBrowsePageStartsForRows({
      rowIndexes: [0],
      columnCount: 4,
      totalRecordCount: 8,
      pageSize: 4,
      reverse: true,
    }),
  ).toEqual([4, 0]);
});

test('library browse page starts omit the reverse look-ahead below page zero', () => {
  expect(
    libraryBrowsePageStartsForRows({
      rowIndexes: [17],
      columnCount: 7,
      totalRecordCount: 125,
      pageSize: 24,
      reverse: true,
    }),
  ).toEqual([0]);
  expect(
    libraryBrowsePageStartsForRows({
      rowIndexes: [1],
      columnCount: 4,
      totalRecordCount: 8,
      pageSize: 4,
      reverse: true,
    }),
  ).toEqual([0]);
});

test('library browse page starts return no pages for an empty window', () => {
  expect(
    libraryBrowsePageStartsForRows({
      rowIndexes: [],
      columnCount: 7,
      totalRecordCount: 125,
      pageSize: 24,
      reverse: false,
    }),
  ).toEqual([]);
});

test('library browse page starts reject invalid geometry', () => {
  for (const columnCount of [0, -2, 1.5, Number.NaN]) {
    expect(
      libraryBrowsePageStartsForRows({
        rowIndexes: [0],
        columnCount,
        totalRecordCount: 125,
        pageSize: 24,
        reverse: false,
      }),
    ).toEqual([]);
  }
  for (const pageSize of [0, -24, 2.5, Number.NaN]) {
    expect(
      libraryBrowsePageStartsForRows({
        rowIndexes: [0],
        columnCount: 7,
        totalRecordCount: 125,
        pageSize,
        reverse: false,
      }),
    ).toEqual([]);
  }
  for (const totalRecordCount of [-1, 2.5, Number.NaN]) {
    expect(
      libraryBrowsePageStartsForRows({
        rowIndexes: [0],
        columnCount: 7,
        totalRecordCount,
        pageSize: 24,
        reverse: false,
      }),
    ).toEqual([]);
  }
  expect(
    libraryBrowsePageStartsForRows({
      rowIndexes: [0],
      columnCount: 7,
      totalRecordCount: 0,
      pageSize: 24,
      reverse: false,
    }),
  ).toEqual([]);
});

test('library browse page starts ignore invalid row indexes', () => {
  expect(
    libraryBrowsePageStartsForRows({
      rowIndexes: [-1, Number.NaN, Number.POSITIVE_INFINITY, 1.5],
      columnCount: 7,
      totalRecordCount: 125,
      pageSize: 24,
      reverse: false,
    }),
  ).toEqual([]);
  expect(
    libraryBrowsePageStartsForRows({
      rowIndexes: [Number.NaN, 3],
      columnCount: 7,
      totalRecordCount: 125,
      pageSize: 24,
      reverse: false,
    }),
  ).toEqual([0, 24, 48]);
});

test('library browse page starts ignore rows beyond the result set', () => {
  expect(
    libraryBrowsePageStartsForRows({
      rowIndexes: [18, 100],
      columnCount: 7,
      totalRecordCount: 125,
      pageSize: 24,
      reverse: false,
    }),
  ).toEqual([]);
});

test('library browse page retention keeps only the active virtual window', () => {
  const pages = new Map([
    [0, 'zero'],
    [24, 'twenty-four'],
    [48, 'forty-eight'],
    [72, 'seventy-two'],
  ]);

  const retained = retainLibraryBrowsePages(pages, new Set([24, 48]));

  expect([...retained]).toEqual([
    [24, 'twenty-four'],
    [48, 'forty-eight'],
  ]);
  expect(retained).not.toBe(pages);
});

test('library browse page retention preserves identity when every page remains active', () => {
  const pages = new Map([
    [24, 'twenty-four'],
    [48, 'forty-eight'],
  ]);

  expect(retainLibraryBrowsePages(pages, new Set([24, 48]))).toBe(pages);
});
