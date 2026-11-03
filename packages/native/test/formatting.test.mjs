import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { formatSource, Compiler } from '../index.js'

const fixture = JSON.parse(readFileSync(new URL('./fixtures/css-prettier-3.6.2.json', import.meta.url)))
test('CSS golden outputs match Prettier 3.6.2', () => {
  assert.equal(fixture.prettierVersion, '3.6.2')
  for (const { input, output } of fixture.fixtures) {
    const result = formatSource(input)
    assert.deepEqual(result.diagnostics, [])
    assert.equal(result.text, output, input)
    assert.equal(formatSource(result.text).text, result.text)
  }
})
test('formatting preserves compiler behavior for XCSS expressions and injections', () => {
  const source = '@vars { --size: length = 2px; } $gap: length = 3px; @injection paint($c: color = red) { color: $c; } .a { @:paint(blue); width: --size * 2; padding: $gap; }'
  const result = formatSource(source)
  assert.deepEqual(result.diagnostics, [])
  const compile = text => {
    const compiler = new Compiler()
    compiler.setFile('app.xcss', 1, text)
    const compiled = compiler.compile('app.xcss', true)
    assert.equal(compiled.success, true, JSON.stringify(compiled.diagnostics))
    return compiled.artifacts.map(({ css, classes, customProperties }) => ({ css, classes, customProperties }))
  }
  assert.deepEqual(compile(result.text), compile(source))
})
test('native options, errors, Unicode positions, and raw bytes', () => {
  const result = formatSource('.a{content:"😀";@raw {\r\n x{ a:   b; }\r\n}}', { useTabs: true, endOfLine: 'crlf' })
  assert.deepEqual(result.diagnostics, [])
  assert(result.text.includes('\r\n\tcontent: "😀";'))
  assert(result.text.includes('{\r\n x{ a:   b; }\r\n}'))
  const broken = formatSource('/* 😀 */ .a {')
  assert.equal(broken.text, undefined)
  assert(broken.diagnostics.length > 0)
  assert(broken.diagnostics.every(d => Number.isInteger(d.range.start.character)))
  assert.throws(() => formatSource('.a{}', { endOfLine: 'invalid' }))
})
