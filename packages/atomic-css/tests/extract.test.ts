import { expect, test } from '@rstest/core'
import { extractAtomicCalls } from '../src/compiler/extract'
import { rewriteAtomicSource } from '../src/compiler/rewrite'
import { assertHostVersions, PINNED_HOSTS } from '../src/compiler/hosts'
import { atomic } from '../src/index'

test('uncompiled atomic marker throws exact message', () => {
  expect(() => atomic({ display: 'flex' })).toThrowError(
    'atomic() must be compiled by pluginAtomic()',
  )
})

test('extracts a minimal static object call', () => {
  const source = `export const box = atomic({ display: 'flex', gap: 4 })`
  const calls = extractAtomicCalls(source)
  expect(calls).toHaveLength(1)
  expect(calls[0]?.declarations).toEqual([
    { property: 'display', value: 'flex' },
    { property: 'gap', value: '1rem' },
  ])
})

test('rejects empty atomic calls', () => {
  expect(() => extractAtomicCalls(`atomic({})`)).toThrowError(
    'atomic() calls must not be empty',
  )
})

test('rewrites atomic() to style() and imports style when missing', () => {
  const source = `export const box = atomic({ display: 'flex' })`
  const { code, rules } = rewriteAtomicSource(source)
  expect(code).toContain("import { style } from '@vanilla-extract/css'")
  expect(code).toContain('style({"display":"flex"})')
  expect(rules).toHaveLength(1)
  expect(rules[0]?.id).toMatch(/^[a-f0-9]{12}$/)
})

test('host version drift fails closed', () => {
  expect(() =>
    assertHostVersions((name) =>
      name === '@rsbuild/core' ? '0.0.0' : PINNED_HOSTS[name],
    ),
  ).toThrowError(/atomic-css host version drift/)
})

test('matching host versions pass', () => {
  assertHostVersions((name) => PINNED_HOSTS[name])
})
