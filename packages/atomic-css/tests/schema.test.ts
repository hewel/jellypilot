import { mkdtempSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { createGenerator } from '@unocss/core'
import { presetMini as unoPresetMini } from '@unocss/preset-mini'
import { expect, test } from '@rstest/core'
import { overrideToken, themeRef } from '../src/config'
import { collectConfigDiagnostics } from '../src/config/load'
import {
  checkGeneratedTypes,
  writeGeneratedTypes,
} from '../src/compiler/types-gen'
import { presetMini } from '../src/preset-mini'
import { buildAtomicSchema, resolveDeclaration } from '../src/schema/schema'

test('presetMini verifies versions and returns frozen layout theme', () => {
  const source = presetMini()
  expect(source.id).toBe('preset-mini')
  expect(source.version).toBe('66.7.4')
  expect(source.selectors.breakpoints).toMatchObject({
    sm: '640px',
    md: '768px',
    lg: '1024px',
    xl: '1280px',
    '2xl': '1536px',
  })
  expect(source.theme.spacing.none).toBe('0')
  expect(source.families.length).toBeGreaterThan(10)
})

test('layout resolve matches preset-mini generator goldens', async () => {
  const uno = await createGenerator({ presets: [unoPresetMini()] })
  const schema = buildAtomicSchema({ preset: 'preset-mini' })

  const cases: Array<{
    atomic: { property: string; value: string | number }
    className: string
    expectCss: string[]
  }> = [
    {
      atomic: { property: 'display', value: 'flex' },
      className: 'flex',
      expectCss: ['display:flex'],
    },
    {
      atomic: { property: 'items', value: 'center' },
      className: 'items-center',
      expectCss: ['align-items:center'],
    },
    {
      atomic: { property: 'justify', value: 'between' },
      className: 'justify-between',
      expectCss: ['justify-content:space-between'],
    },
    {
      atomic: { property: 'gap', value: 4 },
      className: 'gap-4',
      expectCss: ['gap:1rem'],
    },
    {
      atomic: { property: 'p', value: 4 },
      className: 'p-4',
      expectCss: ['padding:1rem'],
    },
    {
      atomic: { property: 'px', value: 2 },
      className: 'px-2',
      expectCss: ['padding-left:0.5rem', 'padding-right:0.5rem'],
    },
    {
      atomic: { property: 'width', value: 'full' },
      className: 'w-full',
      expectCss: ['width:100%'],
    },
    {
      atomic: { property: 'height', value: 'screen' },
      className: 'h-screen',
      expectCss: ['height:100vh'],
    },
    {
      atomic: { property: 'z', value: 10 },
      className: 'z-10',
      expectCss: ['z-index:10'],
    },
    {
      atomic: { property: 'position', value: 'absolute' },
      className: 'absolute',
      expectCss: ['position:absolute'],
    },
  ]

  for (const entry of cases) {
    const resolved = resolveDeclaration(
      schema,
      entry.atomic.property,
      entry.atomic.value,
    )
    const generated = await uno.generate(entry.className, { preflights: false })
    const css = generated.css.replace(/\s+/g, '')
    for (const fragment of entry.expectCss) {
      const compact = fragment.replace(/\s+/g, '')
      expect(css).toContain(compact)
      expect(
        resolved.some(
          (declaration) =>
            `${declaration.cssProperty}:${declaration.value}`.replace(
              /\s+/g,
              '',
            ) === compact,
        ),
      ).toBe(true)
    }
  }
})

test('duplicate Project Theme tokens require overrideToken', () => {
  expect(() =>
    buildAtomicSchema({
      preset: 'preset-mini',
      projectTheme: {
        tokens: { none: '0px' },
      },
    }),
  ).toThrowError(/overrideToken/)

  const schema = buildAtomicSchema({
    preset: 'preset-mini',
    projectTheme: {
      tokens: { none: '0px' },
    },
    overrides: [overrideToken('padding', 'none', '0px')],
  })
  expect(resolveDeclaration(schema, 'padding', 'none')[0]?.value).toBe('0px')
})

test('themeRef stays serializable without executing theme modules', () => {
  const ref = themeRef('./theme.css.ts', 'spaceMd')
  expect(ref).toEqual({
    kind: 'theme-ref',
    moduleId: './theme.css.ts',
    exportName: 'spaceMd',
  })
  const schema = buildAtomicSchema({
    preset: 'preset-mini',
    overrides: [overrideToken('padding', 'brand', ref)],
  })
  expect(resolveDeclaration(schema, 'padding', 'brand')[0]?.value).toBe(
    'var(--atomic-theme-spaceMd)',
  )
})

test('config diagnostics reject prohibited forms and imports', () => {
  const diagnostics = collectConfigDiagnostics(`
    import fs from 'node:fs'
    let x = 1
    export default { preset: 'preset-mini' }
  `)
  expect(diagnostics.some((line) => line.includes('unapproved-import'))).toBe(
    true,
  )
  expect(diagnostics.some((line) => line.includes('mutable-let'))).toBe(true)
})

test('generate and check project types', () => {
  const dir = mkdtempSync(join(tmpdir(), 'atomic-types-'))
  const schema = buildAtomicSchema({ preset: 'preset-mini' })
  const outFile = join(dir, 'atomic-input.d.ts')
  writeGeneratedTypes(schema, outFile)
  expect(checkGeneratedTypes(schema, outFile).ok).toBe(true)
  writeFileSync(outFile, 'stale', 'utf8')
  expect(checkGeneratedTypes(schema, outFile).ok).toBe(false)
})
