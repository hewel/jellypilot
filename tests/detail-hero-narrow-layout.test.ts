import { expect, test } from '@rstest/core';

import { detailHeroNarrowLayout } from '../src/components/library/DetailHero.styles';

test('detail hero uses stacked full-width actions under sm (360 stress)', () => {
  expect(detailHeroNarrowLayout.actionsBaseDirection).toBe('column');
  expect(detailHeroNarrowLayout.actionBaseWidth).toBe('full');
  expect(detailHeroNarrowLayout.contentPaddingXBase).toBe('4');
  expect(detailHeroNarrowLayout.contentBox).toBe('border-box');
  expect(detailHeroNarrowLayout.contentMaxWidth).toBe('[100%]');
  expect(detailHeroNarrowLayout.titleOverflowWrap).toBe('anywhere');
});

test('detail hero restores row actions from sm for supported desktop sizes', () => {
  expect(detailHeroNarrowLayout.actionsSmDirection).toBe('row');
  expect(detailHeroNarrowLayout.actionSmWidth).toBe('auto');
});

test('detail hero root grows with content instead of fixed-height clipping', () => {
  expect(detailHeroNarrowLayout.rootMinHeight.startsWith('[clamp(')).toBe(true);
});
