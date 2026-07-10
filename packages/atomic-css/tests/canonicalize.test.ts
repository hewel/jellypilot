import { expect, test } from '@rstest/core'
import { resolveConflicts } from '../src/compiler/conflicts'
import { extractAtomicCalls } from '../src/compiler/extract'
import {
  buildManifest,
  canonicalRuleId,
} from '../src/compiler/manifest'

test('side beats axis beats all for padding regardless of key order', () => {
  const a = resolveConflicts([
    { property: 'padding', value: '1rem' },
    { property: 'padding-left', value: '2rem' },
  ])
  expect(a.find((d) => d.property === 'padding')).toBeUndefined()
  expect(a.find((d) => d.property === 'padding-left')?.value).toBe('2rem')

  const b = resolveConflicts([
    { property: 'padding-left', value: '2rem' },
    { property: 'padding', value: '1rem' },
  ])
  expect(canonicalRuleId(a)).toBe(canonicalRuleId(b))
})

test('equal-scope different values fail', () => {
  expect(() =>
    resolveConflicts([
      { property: 'padding-left', value: '1rem' },
      { property: 'padding-left', value: '2rem' },
    ]),
  ).toThrowError(/equal-scope conflict/)
})

test('identical declarations collapse', () => {
  const resolved = resolveConflicts([
    { property: 'display', value: 'flex' },
    { property: 'display', value: 'flex' },
  ])
  expect(resolved).toEqual([{ property: 'display', value: 'flex' }])
})

test('canonical ids ignore author key order', () => {
  const left = extractAtomicCalls(
    `atomic({ display: 'flex', gap: 4, p: 2 })`,
  )[0]!
  const right = extractAtomicCalls(
    `atomic({ p: 2, gap: 4, display: 'flex' })`,
  )[0]!
  expect(canonicalRuleId(left.declarations)).toBe(
    canonicalRuleId(right.declarations),
  )
})

test('manifest dedupes cross-file origins and sorts stably', () => {
  const declarations = [
    { property: 'display', value: 'flex' },
    { property: 'gap', value: '1rem' },
  ]
  const manifest = buildManifest([
    { declarations, origin: 'b.css.ts' },
    { declarations: [...declarations].reverse(), origin: 'a.css.ts' },
    { declarations, origin: 'b.css.ts' },
  ])
  expect(manifest.entries).toHaveLength(1)
  expect(manifest.entries[0]?.origins).toEqual(['a.css.ts', 'b.css.ts'])
  expect(manifest.entries[0]?.id).toBe(canonicalRuleId(declarations))
})

test('empty atomic calls fail', () => {
  expect(() => extractAtomicCalls('atomic({})')).toThrowError(/must not be empty/)
})
