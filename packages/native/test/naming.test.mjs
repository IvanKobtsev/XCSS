import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { Compiler } from '../index.js'

test('project configuration controls scoped names and invalidates allocations', () => {
  const root = mkdtempSync(join(tmpdir(), 'xcss-names-'))
  try {
    writeFileSync(join(root, 'xcss.config.json'), JSON.stringify({ suffixLength: 4 }))
    const compiler = new Compiler()
    assert.ok(compiler.configureProject(root).some(path => path.endsWith('xcss.config.json')))
    compiler.setFile('z.xcss', 1, '.button { color: red; }')
    assert.equal(compiler.compile('z.xcss').artifacts[0].classes.button, 'button_0000')
    compiler.setFile('a.xcss', 1, '.button { color: blue; }')
    assert.equal(compiler.compile('z.xcss').artifacts[0].classes.button, 'button_0001')
    compiler.removeFile('a.xcss', 2)
    assert.equal(compiler.compile('z.xcss').artifacts[0].classes.button, 'button_0000')
    writeFileSync(join(root, 'xcss.config.json'), JSON.stringify({ suffixLength: 3 }))
    compiler.configureProject(root)
    assert.equal(compiler.compile('z.xcss').artifacts[0].classes.button, 'button_000')
  } finally { rmSync(root, { recursive: true, force: true }) }
})
