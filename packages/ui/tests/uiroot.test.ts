import { expect, test } from '@rstest/core'
import { defineTheme } from '../src/theme/defineTheme'
import { jellypilotTheme } from '../src/theme/jellypilot'
import { neutralTheme } from '../src/theme/neutral'
import { UIInvariantError } from '../src/runtime/invariant'
import { familyRegistry } from '../src/registry/index'

test('defineTheme rejects incomplete descriptors', () => {
  expect(() =>
    defineTheme({
      id: '',
      fonts: { sans: 'a', mono: 'b' },
      colors: { background: '#fff', foreground: '#000' },
      space: { 1: '4px' },
      radii: { sm: '2px' },
    }),
  ).toThrowError(UIInvariantError)
})

test('Neutral and JellyPilot presets are complete opaque descriptors', () => {
  expect(neutralTheme.id).toBe('neutral')
  expect(neutralTheme.fonts.sans).toContain('system-ui')
  expect(jellypilotTheme.id).toBe('jellypilot')
  expect(jellypilotTheme.fonts.sans).toContain('Figtree')
  expect(jellypilotTheme.colors.background).toBe('#05060a')
})

test('registry includes foundation and document families', () => {
  const names = familyRegistry.map((entry) => entry.exportName)
  for (const required of ['Heading', 'Link', 'Text', 'Theme', 'UIRoot', 'VisuallyHidden']) {
    expect(names).toContain(required)
  }
})
