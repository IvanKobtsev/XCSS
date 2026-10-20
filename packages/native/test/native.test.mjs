import assert from 'node:assert/strict'
import test from 'node:test'

import { transform } from '../index.js'

test('returns transformed code without diagnostics', () => {
  const result = transform('a { width: --size * 2; }', 'css', 'a.cross.css')
  assert.equal(result.code, 'a { width: calc(var(--size) * 2); }')
  assert.deepEqual(result.diagnostics, [])
})

test('returns structured diagnostics', () => {
  const result = transform("@use './tokens';", 'scss', 'тема.cross.scss')
  assert.equal(result.code, null)
  assert.equal(result.diagnostics[0].filename, 'тема.cross.scss')
  assert.equal(result.diagnostics[0].line, 1)
  assert.equal(result.diagnostics[0].column, 1)
  assert.match(result.diagnostics[0].message, /XCSS import system/)
})

test('expands module-scoped vars', () => {
  const result = transform(
    '@vars { --tone: color = red; } a { color: --tone; }',
    'css',
    '/project/src/a.cross.css',
    'src/a.cross.css',
  )
  assert.match(result.code, /@property --tone-xcss-[0-9a-f]{16}/)
  assert.match(result.code, /color: var\(--tone-xcss-[0-9a-f]{16}\)/)
})
