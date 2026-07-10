import { createRoot } from 'solid-js'
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

test('Slider rejects invalid min/max ordering', async () => {
  const { Slider } = await import('../src/components/Slider')
  createRoot((dispose) => {
    expect(() =>
      Slider({
        value: 10,
        min: 20,
        max: 10,
      }),
    ).toThrowError(UIInvariantError)
    dispose()
  })
})

test('SegmentedControl rejects duplicate keys', async () => {
  const { SegmentedControl } = await import('../src/components/SegmentedControl')
  createRoot((dispose) => {
    expect(() =>
      SegmentedControl({
        value: 'a',
        items: [
          { value: 'a', label: 'A' },
          { value: 'a', label: 'A2' },
        ],
      }),
    ).toThrowError(UIInvariantError)
    dispose()
  })
})

test('Tabs rejects unknown controlled value', async () => {
  const { Tabs } = await import('../src/components/Tabs')
  createRoot((dispose) => {
    expect(() =>
      Tabs({
        value: 'missing',
        items: [{ value: 'a', label: 'A', content: 'A body' }],
      }),
    ).toThrowError(UIInvariantError)
    dispose()
  })
})
