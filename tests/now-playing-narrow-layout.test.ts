import { expect, test } from '@rstest/core';

import { nowPlayingCardNarrowLayout } from '../src/components/NowPlayingCard.styles';
import { nowPlayingDrawerNarrowLayout } from '../src/components/NowPlayingDrawer.styles';

test('now playing drawer cannot grow past the viewport at 360 stress width', () => {
  // Flex child of the full-viewport positioner must shrink (minWidth 0).
  expect(nowPlayingDrawerNarrowLayout.contentWidth).toBe('full');
  expect(nowPlayingDrawerNarrowLayout.contentMaxWidth).toBe('[100%]');
  expect(nowPlayingDrawerNarrowLayout.contentMinWidth).toBe('[0]');
  expect(nowPlayingDrawerNarrowLayout.bodyMinWidth).toBe('[0]');
  expect(nowPlayingDrawerNarrowLayout.smWidth).toBe('[28rem]');
});

test('now playing transport controls wrap and play can shrink under sm', () => {
  expect(nowPlayingCardNarrowLayout.controlsBaseWrap).toBe('wrap');
  expect(nowPlayingCardNarrowLayout.controlsBaseGap).toBe('2');
  expect(nowPlayingCardNarrowLayout.playMinWidthBase).toBe('[0]');
  expect(nowPlayingCardNarrowLayout.playMinWidthSm).toBe('[8rem]');
  expect(nowPlayingCardNarrowLayout.rootMinWidth).toBe('[0]');
  expect(nowPlayingCardNarrowLayout.rootMaxWidth).toBe('[100%]');
});
