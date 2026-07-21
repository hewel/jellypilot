import { expect, test } from '@rstest/core';

import {
  breakpoints,
  durations,
  easings,
  fontSizes,
  fontWeights,
  fonts,
  lineHeights,
  radii,
  rawColors,
  semanticColorHex,
  shadows,
  spacing,
  zIndex,
} from '../src/styles/theme-tokens';

test('raw palette values match the Control Room contract', () => {
  expect(rawColors.neutral['975']).toBe('#05060a');
  expect(rawColors.indigo['600']).toBe('#4f46e5');
  expect(rawColors.teal['400']).toBe('#4fe3b1');
  expect(rawColors.amber['400']).toBe('#f6c768');
  expect(rawColors.red['400']).toBe('#ff6b7a');
});

test('semantic colors resolve to the same hex values as before', () => {
  expect(semanticColorHex.primary).toBe('#4f46e5');
  expect(semanticColorHex.background).toBe('#05060a');
  expect(semanticColorHex.onSurface).toBe('#f3f6ff');
  expect(semanticColorHex.surfaceContainerLowest).toBe('#040508');
  expect(semanticColorHex.surfaceTint).toBe(semanticColorHex.primary);
});

test('scale tokens preserve prior keys and values', () => {
  expect(spacing['3_5']).toBe('0.875rem');
  expect(spacing.md).toBe('1rem');
  expect(fontSizes['14']).toBe('0.875rem');
  expect(fontWeights.bold).toBe('700');
  expect(lineHeights['20']).toBe('1.25rem');
  expect(radii['2xl']).toBe('1rem');
  expect(shadows.md).toContain('0.35');
  expect(zIndex['50']).toBe('50');
  expect(durations['200']).toBe('200ms');
  expect(easings.standard).toBe('cubic-bezier(0.2, 0, 0, 1)');
  expect(breakpoints.sm).toBe('640px');
  expect(fonts.sans).toContain('Inter Variable');
});
