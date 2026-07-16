import { expect, test } from '@rstest/core';

import {
  createInitialState,
  matrixEntries,
  nextPendingEntry,
  progressSummary,
  recordResult,
  renderMarkdownReport,
} from '../scripts/tauri-parity-review.mjs';

test('encodes the complete issue 141 screen and viewport matrix', () => {
  const entries = matrixEntries();

  expect(entries).toHaveLength(24);
  expect(entries.filter((entry) => entry.screen === 'login')).toHaveLength(5);
  expect(entries.filter((entry) => entry.screen === 'operations')).toHaveLength(5);
  expect(entries.filter((entry) => entry.screen === 'library-browse')).toHaveLength(5);
  expect(entries.filter((entry) => entry.screen === 'item-detail')).toHaveLength(5);
  expect(entries.filter((entry) => entry.screen === 'library-landing')).toHaveLength(2);
  expect(entries.filter((entry) => entry.screen === 'series-detail')).toHaveLength(2);
});

test('requires evidence and an overflow check before recording a pass', () => {
  const state = createInitialState();

  expect(() =>
    recordResult(state, 'login', '360x720', {
      checks: 'focus',
      evidence: 'login.png',
      verdict: 'pass',
    }),
  ).toThrow("must include the 'overflow' check");

  expect(() =>
    recordResult(state, 'login', '360x720', {
      checks: 'focus,overflow',
      verdict: 'pass',
    }),
  ).toThrow('requires --evidence');
});

test('tracks progress and renders a PR-ready evidence report', () => {
  const initial = createInitialState();
  const state = recordResult(initial, 'login', '360x720', {
    checks: 'focus,overflow,portals',
    evidence: 'https://example.test/login-360.png',
    notes: 'No clipping; focus returned after closing the dialog.',
    states: 'validation-error,quick-connect-waiting',
    verdict: 'pass',
  });

  expect(progressSummary(state)).toEqual({ failed: 0, passed: 1, pending: 23, total: 24 });
  expect(nextPendingEntry(state)?.id).toBe('login@640x720');

  const report = renderMarkdownReport(state);
  expect(report).toContain('**1/24 passed**');
  expect(report).toContain('| Login | 360x720 | PASS |');
  expect(report).toContain('[link](https://example.test/login-360.png)');
  expect(report).toContain('- [x] Keyboard focus');
  expect(report).toContain('- [ ] Reduced motion');
});
