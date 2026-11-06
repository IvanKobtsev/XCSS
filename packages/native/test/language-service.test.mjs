import assert from 'node:assert/strict'
import { test } from 'node:test'
import { LanguageService, languageServiceCapabilities } from '../index.js'

test('TS checking accepts unsaved project sources and updates after XCSS edits', () => {
  const service = new LanguageService()
  service.setTypescriptProject('tsconfig.json')
  service.setTypescriptFile('tsconfig.json', 1, JSON.stringify({ compilerOptions: { strict: true }, files: ['index.ts'] }))
  service.setFile('a.xcss', 1, '@variants V: .one; .one {}')
  service.setTypescriptFile('index.ts', 1, "import s, { type Variants } from './a.xcss'; const v: Variants<'V'> = 'two'; s.missing;")
  assert.equal(service.document('index.ts').diagnostics.length, 2)
  service.setFile('a.xcss', 2, '@variants V: .two; .two {} .missing {}')
  assert.deepEqual(service.document('index.ts').diagnostics, [])
  service.removeTypescriptFile('index.ts', 2)
  assert.throws(() => service.setTypescriptFile('index.ts', 2, ''), /StaleRevision/)
})

test('editor API returns UTF-16 facts without invoking compilation', () => {
  const service = new LanguageService()
  assert.equal(languageServiceCapabilities().apiVersion, 4)
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


test('line comments do not create symbols or editor tokens', () => {
 const service = new LanguageService()
 const uri = 'file:///comments.xcss'
 service.setFile(uri, 1, '// 😀 .fake { @:missing; }\n@vars { --gap = 0; }\n.real // .also-fake\n{ margin: --gap; }')
 const document = service.document(uri)
 assert.deepEqual(document.diagnostics, [])
 assert(!document.tokens.some(token => token.line === 0))
 assert(!document.tokens.some(token => token.line === 2 && token.character >= 6))
 assert.match(service.hover(uri, { line: 1, character: 11 }).text, /length/)
})
