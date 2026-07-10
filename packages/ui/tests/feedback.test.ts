import { expect, test } from '@rstest/core'
import { familyRegistry } from '../src/registry/index'

test('feedback families are registered', () => {
  const names = familyRegistry.map((entry) => entry.exportName)
  for (const required of ['Card', 'Badge', 'Spinner', 'Skeleton', 'ProgressBar']) {
    expect(names).toContain(required)
  }
})
