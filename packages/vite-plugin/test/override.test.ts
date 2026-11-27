import { expect, test } from 'vitest'
import { applyOverride, override, resolveVariants } from '../src/override.js'
import { normalizeProps } from '../src/react/props.js'
import type { ClassName } from '../src/types.js'

test('compiled override calls retain originals and reject unexpected or repeated values', () => {
  const entries = [['button utility', 'button utility button_override_2 utility_override_2']] as const
  const value = applyOverride('button utility' as ClassName, entries)
  expect(value).toBe(entries[0][1])
  expect(() => applyOverride(value, entries)).toThrow(/already-overridden/)
  expect(() => applyOverride('unknown' as ClassName, entries)).toThrow(/unexpected/)
  expect(() => override('button' as ClassName)).toThrow(/compiled/)
})

test('xClass accepts nested and dynamic clsx inputs without metadata', () => {
  const props = normalizeProps('div', { className: 'global', xClass: ['a', 0, 3, [null, false, { b: true, c: false }]] })
  expect(props).toEqual({ className: 'global a 3 b' })
  expect(resolveVariants(['primary', { secondary: true }], [['primary', 'primary_000'], ['secondary', 'secondary_000']])).toEqual(['primary_000', { secondary_000: true }])
})
