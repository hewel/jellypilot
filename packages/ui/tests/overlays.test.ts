import { expect, test } from '@rstest/core'
import { familyRegistry } from '../src/registry/index'

test('overlay families are registered', () => {
  const names = familyRegistry.map((entry) => entry.exportName)
  for (const required of ['Collapsible', 'Dialog', 'AlertDialog', 'Popover']) {
    expect(names).toContain(required)
  }
})
