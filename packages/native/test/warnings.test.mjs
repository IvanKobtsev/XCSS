import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { spawnSync } from 'node:child_process'
const require = createRequire(import.meta.url)
const { LanguageService } = require('../index.js')
const { validateCss } = require('../css-validation.cjs')

test('empty and invalid CSS values are uncoded warnings at source ranges', () => {
  const service = new LanguageService()
  service.setFile('a.xcss', 1, '/* 😀 */\n.example { width: ; z-index: abc; --custom: ; }')
  const warnings = validateCss(service.cssDeclarations('a.xcss'))
  assert.equal(warnings.length, 2)
  assert(warnings.every(w => w.severity === 2 && !('code' in w) && w.range.start.line === 1))
  assert.match(warnings[0].message, /Empty CSS value/)
  assert.match(warnings[1].message, /z-index/)
})

test('CLI warnings do not fail checks; coded errors still do', () => {
  const root = mkdtempSync(join(tmpdir(), 'xcss-warnings-'))
  try {
    writeFileSync(join(root, 'a.xcss'), '.empty {} .invalid { width: ; z-index: abc; }')
    const script = `process.exitCode = require(${JSON.stringify(resolve('cli.cjs'))})(['check'])`
    const result = spawnSync(process.execPath, ['-e', script], { cwd: root, encoding: 'utf8' })
    assert.equal(result.status, 0, result.stderr)
    assert.match(result.stderr, /Warnings:/)
    assert.match(result.stderr, /empty declaration/)
    assert.match(result.stderr, /Empty CSS value/)
    assert.match(result.stderr, /z-index/)
    writeFileSync(join(root, 'a.xcss'), 'bogus;')
    const failure = spawnSync(process.execPath, ['-e', script], { cwd: root, encoding: 'utf8' })
    assert.equal(failure.status, 1)
    assert.match(failure.stderr, /XC-105/)
  } finally { rmSync(root, { recursive: true, force: true }) }
})
