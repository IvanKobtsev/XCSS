import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { Compiler } from '../index.js'

test('xClass positions do not allocate names or change CSS', () => {
  const compiler = new Compiler()
  compiler.setFile('a.xcss', 1, '.a { color: red; } .b { color: blue; }')
  const before = compiler.compile('a.xcss').artifacts[0]
  const plan = tuple => compiler.planClassUsage('tsconfig.json', [
    { path: 'tsconfig.json', text: JSON.stringify({ compilerOptions: { strict: true, jsx: 'preserve', module: 'esnext', moduleResolution: 'bundler' }, files: ['app.tsx'] }) },
    { path: 'app.tsx', text: `import { styles } from './a.xcss'; const element = <div xClass={${tuple}} />;` },
  ])
  for (const tuple of ['[styles.a, styles.b]', '[styles.b, false, styles.a]']) {
    const result = plan(tuple)
    assert.equal(result.diagnostics.length, 0)
    assert.equal(result.sites.length, 0)
    const after = compiler.compile('a.xcss').artifacts[0]
    assert.deepEqual(after.classes, before.classes)
    assert.equal(after.css, before.css)
  }
})

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
