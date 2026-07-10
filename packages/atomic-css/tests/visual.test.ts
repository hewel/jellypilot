import { createGenerator } from '@unocss/core'
import { presetMini as unoPresetMini } from '@unocss/preset-mini'
import { expect, test } from '@rstest/core'
import { overrideToken } from '../src/config'
import { presetMini } from '../src/preset-mini'
import { buildAtomicSchema, resolveDeclaration } from '../src/schema/schema'

test('flattens colors and rejects legacy numeric aliases', () => {
  const source = presetMini()
  expect(source.theme.colors['blue-500']).toBe('#3b82f6')
  expect(source.theme.colors.blue).toBe('#60a5fa')
  expect(source.unsupportedColorAliases).toContain('blue-1')
  expect(source.theme.colors['blue-1']).toBeUndefined()
})

test('fontSize uses tuple zero only', () => {
  const source = presetMini()
  expect(source.theme.fontSize.sm).toBe('0.875rem')
  const schema = buildAtomicSchema({ preset: 'preset-mini' })
  expect(resolveDeclaration(schema, 'fontSize', 'sm')[0]).toEqual({
    cssProperty: 'font-size',
    value: '0.875rem',
  })
})

test('visual families resolve against preset-mini goldens', async () => {
  const uno = await createGenerator({ presets: [unoPresetMini()] })
  const schema = buildAtomicSchema({ preset: 'preset-mini' })

  const cases: Array<{
    atomic: { property: string; value: string | number }
    className?: string
    expectCss: string[]
    compareUno?: boolean
  }> = [
    {
      atomic: { property: 'fontWeight', value: 'bold' },
      className: 'font-bold',
      expectCss: ['font-weight:700'],
      compareUno: true,
    },
    {
      atomic: { property: 'leading', value: 'tight' },
      className: 'leading-tight',
      expectCss: ['line-height:1.25'],
      compareUno: true,
    },
    {
      atomic: { property: 'textAlign', value: 'center' },
      className: 'text-center',
      expectCss: ['text-align:center'],
      compareUno: true,
    },
    {
      atomic: { property: 'rounded', value: 'lg' },
      className: 'rounded-lg',
      expectCss: ['border-radius:0.5rem'],
      compareUno: true,
    },
    {
      atomic: { property: 'opacity', value: 50 },
      className: 'opacity-50',
      expectCss: ['opacity:0.5'],
      compareUno: true,
    },
    {
      atomic: { property: 'color', value: 'blue-500' },
      expectCss: ['color:#3b82f6'],
    },
    {
      atomic: { property: 'bg', value: 'red-500' },
      expectCss: ['background-color:#ef4444'],
    },
    {
      atomic: { property: 'borderColor', value: 'green-600' },
      expectCss: ['border-color:#16a34a'],
    },
    {
      atomic: {
        property: 'shadow',
        value: '0 1px 2px rgb(0 0 0 / 0.1)',
      },
      expectCss: ['box-shadow:0 1px 2px rgb(0 0 0 / 0.1)'],
    },
    {
      atomic: { property: 'gridCols', value: '1fr 2fr' },
      expectCss: ['grid-template-columns:1fr 2fr'],
    },
  ]

  for (const entry of cases) {
    const resolved = resolveDeclaration(
      schema,
      entry.atomic.property,
      entry.atomic.value,
    )
    for (const fragment of entry.expectCss) {
      const compact = fragment.replace(/\s+/g, '')
      expect(
        resolved.some(
          (declaration) =>
            `${declaration.cssProperty}:${declaration.value}`.replace(
              /\s+/g,
              '',
            ) === compact,
        ),
      ).toBe(true)
      if (entry.compareUno && entry.className) {
        const generated = await uno.generate(entry.className, {
          preflights: false,
        })
        expect(generated.css.replace(/\s+/g, '')).toContain(compact)
      }
    }
  }
})

test('negative diagnostics for invalid tokens and arbitrary syntax', () => {
  const schema = buildAtomicSchema({ preset: 'preset-mini' })
  expect(() => resolveDeclaration(schema, 'color', 'blue-999')).toThrowError(
    /invalid-theme-token/,
  )
  expect(() =>
    resolveDeclaration(schema, 'shadow', '0 1px red; color: red'),
  ).toThrowError(/malformed-arbitrary/)
  expect(() =>
    resolveDeclaration(schema, 'opacity', 101),
  ).toThrowError(/opacity must be integer/)
  expect(() =>
    resolveDeclaration(schema, 'color', 'blue-500 !important'),
  ).toThrowError(/!important/)
})

test('project theme shadow tokens are accepted; preset shadows unsupported', () => {
  const schema = buildAtomicSchema({
    preset: 'preset-mini',
    overrides: [
      overrideToken('boxShadow', 'card', '0 4px 6px rgb(0 0 0 / 0.1)'),
    ],
  })
  expect(resolveDeclaration(schema, 'shadow', 'card')[0]?.value).toBe(
    '0 4px 6px rgb(0 0 0 / 0.1)',
  )
  expect(() => resolveDeclaration(schema, 'shadow', 'md')).toThrowError(
    /invalid-theme-token|unsupported value|malformed/,
  )
})
