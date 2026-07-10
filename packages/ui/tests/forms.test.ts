import { expect, test } from '@rstest/core'
import { familyRegistry } from '../src/registry/index'
import { UIInvariantError } from '../src/runtime/invariant'

test('form and collection families are registered', () => {
  const names = familyRegistry.map((entry) => entry.exportName)
  for (const required of [
    'TextInput',
    'TextArea',
    'CheckboxInput',
    'Switch',
    'Slider',
    'SegmentedControl',
    'Tabs',
  ]) {
    expect(names).toContain(required)
  }
})

test('UIInvariantError codes are stable for form families', () => {
  expect(new UIInvariantError('slider-range', 'x').code).toBe('slider-range')
  expect(new UIInvariantError('segmented-duplicate-key', 'x').code).toBe(
    'segmented-duplicate-key',
  )
  expect(new UIInvariantError('tabs-unknown-value', 'x').code).toBe(
    'tabs-unknown-value',
  )
})
