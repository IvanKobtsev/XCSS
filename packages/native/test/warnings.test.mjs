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

test('empty and invalid CSS values have stable codes at source ranges', () => {
  const service = new LanguageService()
  service.setFile('a.xcss', 1, '/* 😀 */\n.example { width: ; z-index: abc; --custom: ; }')
  const warnings = validateCss(service.cssDeclarations('a.xcss'))
  assert.equal(warnings.length, 2)
  assert(warnings.every(w => w.severity === 2 && w.range.start.line === 1))
  assert.deepEqual(warnings.map(w => w.code), ['XC-502', 'XC-501'])
  assert.match(warnings[0].message, /Empty CSS value/)
  assert.match(warnings[1].message, /z-index/)
})

test('native suppression metadata filters only matching CSS warning start lines', () => {
  const service = new LanguageService()
  const source = '/* 😀 */\r\n.a {\r\n // @xcss-ignore xc-500, xc-502\r\n dispay: flex; width: ; z-index: abc;\r\n // @xcss-ignore xc-501\r\n z-index: abc;\r\n // @xcss-ignore xc-501\r\n\r\n z-index: abc;\r\n}'
  service.setFile('a.xcss', 1, source)
  const input = service.cssDeclarations('a.xcss')
  assert.deepEqual(input.suppressions, [
    { line: 3, codes: ['XC-500', 'XC-502'] },
    { line: 5, codes: ['XC-501'] },
    { line: 7, codes: ['XC-501'] },
  ])
  const warnings = validateCss(input)
  assert.deepEqual(warnings.map(w => [w.code, w.range.start.line]), [['XC-501', 3], ['XC-501', 8]])
  const { suppressions, ...legacyInput } = input
  assert.equal(validateCss(legacyInput).length, 5)
  service.setFile('a.xcss', 2, source.replaceAll('@xcss-ignore', 'ordinary comment'))
  assert.equal(validateCss(service.cssDeclarations('a.xcss')).length, 5)
})

test('typed CSS values use XC-501 and honor suppression', () => {
  const service = new LanguageService()
  service.setFile('a.xcss', 1, '@vars { --t: time = 1s; }\n.a {\n// @xcss-ignore xc-501\nwidth: --t;\nheight: --t;\n}')
  const warnings = validateCss(service.cssDeclarations('a.xcss'))
  assert.equal(warnings.length, 1)
  assert.equal(warnings[0].code, 'XC-501')
  assert.equal(warnings[0].range.start.line, 4)
  assert.match(warnings[0].message, /type “time”/)
})

test('Node checker diagnostics and class warnings honor suppression', () => {
  const service = new LanguageService()
  service.setFile('a.xcss', 1, '@injection f($p: number, $q: number) {}\n.a {\n// @xcss-ignore xc-303, xc-302\n@:f(red); @:missing();\n}\n// @xcss-ignore xc-503\n.hidden {}\n.visible {}')
  assert.deepEqual(service.document('a.xcss').diagnostics.map(d => d.code), ['XC-212'])
  const warnings = service.classWarnings('a.xcss')
  assert.equal(warnings.length, 1)
  assert.equal(warnings[0].code, 'XC-503')
  assert.equal(warnings[0].category, 'empty-class')
  assert.match(warnings[0].message, /visible/)
})

test('Node CLI prints warning codes and applies suppression before determining success', () => {
  const root = mkdtempSync(join(tmpdir(), 'xcss-suppression-'))
  try {
    const script = `process.exitCode = require(${JSON.stringify(resolve('cli.cjs'))})(['check'])`
    const run = () => spawnSync(process.execPath, ['-e', script], { cwd: root, encoding: 'utf8' })
    writeFileSync(join(root, 'a.xcss'), '@injection f($p: number) {}\n.a {\n// @xcss-ignore xc-303\n@:f(red);\n// @xcss-ignore xc-500, xc-502\ndispay: flex; width: ;\nz-index: abc;\n}\n// @xcss-ignore xc-503\n.hidden {}')
    const result = run()
    assert.equal(result.status, 0, result.stderr)
    assert.match(result.stderr, /Warning: XC-501/)
    for (const code of ['XC-303', 'XC-500', 'XC-502', 'XC-503']) assert(!result.stderr.includes(code), result.stderr)
    writeFileSync(join(root, 'a.xcss'), '// @xcss-ignore xc-212\n.a { @:missing(); }\n.b { @:missing(); }')
    const failure = run()
    assert.equal(failure.status, 1, failure.stderr)
    assert.equal((failure.stderr.match(/XC-212/g) ?? []).length, 1, failure.stderr)
  } finally { rmSync(root, { recursive: true, force: true }) }
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
