import { expect, test } from '@rstest/core'
import { familyRegistry } from '../src/registry/index'
import { neutralBreakpoints, neutralTokenValues } from '../src/theme/tokens'

test('registry lists foundation families', () => {
  expect(familyRegistry.map((entry) => entry.exportName).sort()).toEqual([
    'Heading',
    'Text',
    'VisuallyHidden',
  ])
})

test('neutral breakpoints are plain build-time values', () => {
  expect(neutralBreakpoints.md).toBe('768px')
  expect(neutralBreakpoints.md.startsWith('var(')).toBe(false)
})

test('neutral tokens preserve Astryx CSS variable identifiers', () => {
  expect(neutralTokenValues.colors.background).toContain('--colors-background')
  expect(neutralTokenValues.space[4]).toContain('--space-4')
  expect(neutralTokenValues.font.sans).toContain('--font-sans')
})
