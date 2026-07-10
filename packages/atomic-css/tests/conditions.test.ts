import { expect, test } from '@rstest/core'
import {
  canonicalizeConditions,
  conditionSelector,
} from '../src/compiler/conditions'
import { composeTransforms, TRANSFORM_PREFLIGHT } from '../src/compiler/transforms'
import { applyWatchEdit, createWatchState } from '../src/compiler/watch'

test('condition nesting order canonicalizes identically', () => {
  const a = canonicalizeConditions(['hover', 'sm', 'dark'])
  const b = canonicalizeConditions(['dark', 'sm', 'hover'])
  expect(a.map((c) => c.key)).toEqual(b.map((c) => c.key))
  expect(a.map((c) => c.key)).toEqual(['sm', 'hover', 'dark'])
})

test('duplicate predicates collapse; incompatible breakpoints fail', () => {
  expect(canonicalizeConditions(['hover', 'hover']).map((c) => c.key)).toEqual([
    'hover',
  ])
  expect(() => canonicalizeConditions(['sm', 'md'])).toThrowError(
    /incompatible-breakpoints/,
  )
})

test('breakpoints use plain build-time values', () => {
  const conditions = canonicalizeConditions(['md', 'hover'])
  const result = conditionSelector(conditions, {
    breakpoints: { md: '768px' },
    dark: "[data-theme='dark'] &",
  })
  expect(result.media).toBe('(min-width: 768px)')
  expect(result.selectorSuffix).toContain(':hover')
  expect(() =>
    conditionSelector(canonicalizeConditions(['md']), {
      breakpoints: { md: 'var(--bp-md)' },
    }),
  ).toThrowError(/plain build-time/)
})

test('transforms compose axes and reject base/axis overlap', () => {
  const composed = composeTransforms([
    { axis: 'translateX', value: '1rem' },
    { axis: 'rotate', value: '45deg' },
  ])
  expect(composed.properties['--un-translate-x']).toBe('1rem')
  expect(composed.properties['--un-rotate']).toBe('45deg')
  expect(composed.properties.transform).toContain('translate(')
  expect(TRANSFORM_PREFLIGHT).toContain('--un-translate-x: 0')
  expect(() =>
    composeTransforms([
      { axis: 'translate', value: '1rem' },
      { axis: 'translateX', value: '2rem' },
    ]),
  ).toThrowError(/transform-overlap/)
})

test('watch recovery retains last-good assets through invalid edit', () => {
  let state = createWatchState()
  state = applyWatchEdit(state, {
    ok: true,
    assets: { css: '.a{}', manifest: '{"v":1}' },
  })
  state = applyWatchEdit(state, {
    ok: false,
    diagnostics: ['b-error', 'a-error'],
  })
  expect(state.lastGood).toEqual({ css: '.a{}', manifest: '{"v":1}' })
  expect(state.lastDiagnostics).toEqual(['a-error', 'b-error'])
  state = applyWatchEdit(state, {
    ok: true,
    assets: { css: '.b{}', manifest: '{"v":2}' },
  })
  expect(state.lastGood?.css).toBe('.b{}')
  expect(state.lastDiagnostics).toEqual([])
})
