import { expect, test } from '@rstest/core'
import { familyRegistry } from '../src/registry/index'
import { UIInvariantError } from '../src/runtime/invariant'

test('action families are registered', () => {
  const names = familyRegistry.map((entry) => entry.exportName)
  expect(names).toContain('Button')
  expect(names).toContain('IconButton')
  expect(names).toContain('ToggleButton')
})

test('IconButton invariant requires aria-label', async () => {
  // Import after registry assertion; call factory with empty label.
  const { IconButton } = await import('../src/components/IconButton')
  expect(() =>
    IconButton({
      'aria-label': '   ',
      children: 'x',
    }),
  ).toThrowError(UIInvariantError)
})
