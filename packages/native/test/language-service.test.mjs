import assert from 'node:assert/strict'
import { test } from 'node:test'
import { LanguageService, languageServiceCapabilities } from '../index.js'

test('editor API returns UTF-16 facts without invoking compilation', () => {
  const service = new LanguageService()
  assert.equal(languageServiceCapabilities().apiVersion, 2)
  const uri = 'file:///app.xcss'
  const source = '/* 😀 */ @import lib from "./lib"; .a { @:lib.paint(); color: --global; }'
  service.setFile('file:///lib.xcss', 1, '@injection paint($p: color = red) { color: $p; }')
  service.setFile(uri, 1, source)
  service.setResolution(uri, './lib', 'file:///lib.xcss')
  const position = { line: 0, character: source.indexOf('paint') }
  const document = service.document(uri)
  assert.deepEqual(document.diagnostics, [])
  assert.equal(service.definition(uri, position)[0].uri, 'file:///lib.xcss')
  assert.equal(service.references(uri, position, false).length, 1)
  assert.match(service.hover(uri, position).text, /\$p: color = red/)
  const legend = languageServiceCapabilities().tokenTypes
  assert(document.tokens.some(token => legend[token.tokenType] === 'namespace'))
  for (let i = 1; i < document.tokens.length; i++) {
    const a = document.tokens[i - 1], b = document.tokens[i]
    assert(a.line < b.line || a.line === b.line && a.character + a.length <= b.character)
  }
  service.removeFile('file:///lib.xcss', 2)
  assert(service.document(uri).diagnostics.length)
  assert.throws(() => service.setFile(uri, 1, ''), /StaleRevision/)
})
