import { expect, test } from '@rstest/core'
import { box } from './styles.css.ts'

test('rstest compiles atomic() to a class string', () => {
  expect(typeof box).toBe('string')
  expect(box.length).toBeGreaterThan(0)
})
