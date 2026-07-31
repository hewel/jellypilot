import { expect, test } from '@rstest/core';

import { collectMotionInvariantErrors } from '../scripts/check-styling-boundaries.mjs';

test('layout property in transitionProperty is flagged', () => {
  const source = `
export const sliderRange = css({
  borderRadius: 'full',
  height: 'full',
  transitionDuration: '150',
  transitionProperty: '[width, transform]',
});
`;
  const errors = collectMotionInvariantErrors('test.styles.ts', source);
  expect(errors).toEqual([
    "test.styles.ts: layout property 'width' must not appear in transitionProperty (sliderRange)",
  ]);
});

test('interactive transform without resting value is flagged', () => {
  const source = `
export const card = css({
  transitionDuration: '200',
  transitionProperty: '[transform]',
  _hover: {
    transform: '[scale(1.06)]',
  },
});
`;
  const errors = collectMotionInvariantErrors('test.styles.ts', source);
  expect(errors).toEqual([
    "test.styles.ts: interactive-state 'transform' has no resting value (card)",
  ]);
});

test('interactive transform with resting value passes', () => {
  const source = `
export const card = css({
  transform: '[scale(1)]',
  transitionDuration: '200',
  transitionProperty: '[transform]',
  _hover: {
    transform: '[scale(1.06)]',
  },
});
`;
  expect(collectMotionInvariantErrors('test.styles.ts', source)).toEqual([]);
});

test('cva variant transform with base resting value passes; without it fails', () => {
  const passing = `
export const button = cva({
  base: {
    transform: '[scale(1)]',
    transitionProperty: '[background-color, transform]',
  },
  variants: {
    variant: {
      primary: {
        _active: {
          transform: '[scale(0.96)]',
        },
      },
    },
  },
});
`;
  expect(collectMotionInvariantErrors('test.styles.ts', passing)).toEqual([]);

  const failing = `
export const button = cva({
  base: {
    transitionProperty: '[background-color, transform]',
  },
  variants: {
    variant: {
      primary: {
        _active: {
          transform: '[scale(0.96)]',
        },
      },
    },
  },
});
`;
  expect(collectMotionInvariantErrors('test.styles.ts', failing)).toEqual([
    "test.styles.ts: interactive-state 'transform' has no resting value (button)",
  ]);
});

test('paired data-state transforms pass', () => {
  const source = `
export const content = css({
  transitionProperty: '[opacity, transform]',
  '&[data-state="closed"]': {
    opacity: '[0]',
    transform: '[translateY(0.25rem)]',
  },
  '&[data-state="open"]': {
    opacity: '[1]',
    transform: '[translateY(0)]',
  },
});
`;
  expect(collectMotionInvariantErrors('test.styles.ts', source)).toEqual([]);
});

test('unpaired data-state transform is flagged', () => {
  const source = `
export const indicatorIcon = css({
  transitionProperty: '[transform]',
  '[data-state=open] &': {
    transform: '[rotate(180deg)]',
  },
});
`;
  expect(collectMotionInvariantErrors('test.styles.ts', source)).toEqual([
    "test.styles.ts: interactive-state 'transform' has no resting value (indicatorIcon)",
  ]);
});

test('token-string braces do not break depth tracking', () => {
  const source = `
export const thumb = css({
  boxShadow: '[0 10px 15px -3px {colors.secondary/15}]',
  transform: '[scale(1)]',
  transitionProperty: '[box-shadow, transform]',
  _hover: {
    transform: '[scale(1.1)]',
  },
});
`;
  expect(collectMotionInvariantErrors('test.styles.ts', source)).toEqual([]);
});

test('paint-only transition properties pass', () => {
  const source = `
export const input = css({
  transitionProperty: '[background-color, border-color, box-shadow]',
  _hover: {
    bg: 'surface/90',
  },
});
`;
  expect(collectMotionInvariantErrors('test.styles.ts', source)).toEqual([]);
});
