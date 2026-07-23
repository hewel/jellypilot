import { expect, test } from '@rstest/core';

import { appSidebarViewportLayout } from '../src/components/AppSidebar.styles';
import {
  authenticatedShellCollapsedTrackLayout,
  authenticatedShellLayout,
} from '../src/components/AuthenticatedShell.styles';
import { appScrollContentLayout, appScrollViewportLayout } from '../src/routes/__root.styles';

test('app scroll viewport avoids 100vw and fit-content growth', () => {
  expect(appScrollViewportLayout.width).toBe('full');
  expect(appScrollViewportLayout.maxWidth).toBe('[100%]');
  // Overflow-x stays auto so layout regressions remain visible instead of being clipped.
  expect(appScrollViewportLayout.overflowX).toBe('auto');
  expect(appScrollViewportLayout.overflowY).toBe('auto');
  expect(JSON.stringify(appScrollViewportLayout)).not.toContain('100vw');
});

test('app scroll content can shrink below fit-content width', () => {
  expect(appScrollContentLayout.minWidth).toBe('[0]');
  expect(appScrollContentLayout.width).toBe('full');
  expect(appScrollContentLayout.maxWidth).toBe('[100%]');
  expect(JSON.stringify(appScrollContentLayout)).not.toContain('fit');
});

test('authenticated shell isolates the Sidebar from virtual scrolling', () => {
  expect(appSidebarViewportLayout.position).toBe('fixed');
  expect(appSidebarViewportLayout.top).toBe('0');
  expect(appSidebarViewportLayout.left).toBe('0');
  expect(appSidebarViewportLayout.height).toBe('[100dvh]');

  expect(authenticatedShellLayout.gridTemplateColumns).toBe('[4rem minmax(0, 1fr)]');
  expect(authenticatedShellLayout.lg.gridTemplateColumns).toBe('[16rem minmax(0, 1fr)]');
  expect(authenticatedShellCollapsedTrackLayout.lg.gridTemplateColumns).toBe(
    '[4.5rem minmax(0, 1fr)]',
  );
});
