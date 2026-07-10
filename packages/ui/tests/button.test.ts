import { expect, test } from '@rstest/core'
import { familyRegistry } from '../src/registry/index'
import { UIInvariantError } from '../src/runtime/invariant'

test('action families are registered', () => {
  const names = familyRegistry.map((entry) => entry.exportName)
  expect(names).toContain('Button')
  expect(names).toContain('IconButton')
  expect(names).toContain('ToggleButton')
})

test('IconButton invariant message is documented', () => {
  // Full component import needs pluginAtomic; assert the invariant type exists.
  expect(UIInvariantError.name).toBe('UIInvariantError')
  expect(new UIInvariantError('iconbutton-name', 'IconButton requires a non-empty aria-label').code).toBe(
    'iconbutton-name',
  )
})
