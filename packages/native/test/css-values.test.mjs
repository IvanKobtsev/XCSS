import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { LanguageService, languageServiceCapabilities } from '../index.js'

test('reported TitleButton source remains formattable and preserves invalid CSS for the host', async () => {
  const source = readFileSync(new URL('../../../fixtures/editor/title-button.xcss', import.meta.url), 'utf8')
  const service = new LanguageService()
  service.setFile('file:///title.xcss', 1, source)
  const { formatSource } = await import('../index.js')
  assert.equal(formatSource(source).diagnostics.length, 0)
  const declarations = service.cssDeclarations('file:///title.xcss').declarations
  assert(declarations.find(d => d.property === 'mask-image').parts.some(p => p.text === 'transprent'))
  assert(declarations.find(d => d.property === 'width' && d.parts.some(p => p.text === '10ss')).checkable)
})

test('CSS editor values retain gradients, arithmetic, defaults and original UTF-16 ranges', () => {
  const source = '/* 😀 */ $gap = 20px; $tone = red; @vars { --accent: color = #fff; } @injection paint($c: color = rgb(1 2 3)) { color: $c; } .a { mask-image: linear-gradient(black (100% - 350px), transprent 100%); background: linear-gradient(red ($gap + 2px), blue); transform: translateY(-25.5px); }'
  const service = new LanguageService()
  service.setFile('file:///values.xcss', 1, source)
  const declarations = service.cssDeclarations('file:///values.xcss').declarations
  const gradient = declarations.find(d => d.property === 'mask-image')
  assert(gradient.checkable)
  assert(gradient.parts.some(p => p.text === 'transprent'))
  assert(gradient.parts.some(p => p.valueType === 'length-percentage'))
  assert(!gradient.parts.some(p => p.valueType === 'image'))
  assert(declarations.find(d => d.property === 'background').checkable)
  const result = service.cssValues('file:///values.xcss')
  for (const text of ['red', '#fff', 'rgb(1 2 3)']) assert(result.values.some(v => v.text.trim() === text), text)
  const transform = result.values.find(v => v.property === 'transform')
  assert.equal(transform.functions[0].name, 'translateY')
  assert.equal(transform.functions[0].range.start.character, source.indexOf('translateY'))
  assert.equal(transform.functions[0].range.end.character, source.indexOf('translateY') + 'translateY'.length)
  assert.deepEqual(languageServiceCapabilities().tokenTypes.slice(-6, -3), ['cssFunction', 'cssKeyword', 'cssColor'])
})

test('raw contents, comments, strings and URL contents never produce function occurrences', () => {
  const service = new LanguageService()
  service.setFile('file:///values.xcss', 1, '@raw { .a { color: rgb(1 2 3); } } .b { content: "blur(red)"; background: url(blur(red)); filter: blur(2px); }')
  const result = service.cssValues('file:///values.xcss')
  assert.equal(result.values.length, 3)
  assert.deepEqual(result.values.flatMap(v => v.functions.map(f => f.name)), ['url', 'blur'])
  const old = result.revision
  service.setFile('file:///values.xcss', 2, '.b { color: blue; }')
  assert(service.cssValues('file:///values.xcss').revision > old)
  assert.equal(result.values.length, 3)
})
