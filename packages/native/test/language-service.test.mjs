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

test('stylesheet diagnostics and TypeScript setup warnings are independent', () => {
  const service = new LanguageService()
  const uri = 'a.xcss'
  const source = '/* 😀 */ bogus;\n@variants V: .missing;\n@vars { --x: mystery; }\n.a { @:missing; }'
  service.setFile(uri, 1, source)
  service.setTypescriptFile('index.ts', 1, '')
  assert.equal(service.typescriptProjectWarning(), null)
  service.setTypescriptProject('tsconfig.json')
  assert.match(service.typescriptProjectWarning(), /tsconfig\.json/)
  service.setTypescriptFile('tsconfig.json', 1, '{ invalid:')
  assert.match(service.typescriptProjectWarning(), /Property value/)
  assert.deepEqual(service.document('tsconfig.json').diagnostics, [])
  assert.deepEqual(service.document('index.ts').diagnostics, [])

  const broken = service.document(uri).diagnostics
  const statement = broken.find(diagnostic => diagnostic.code === 'XC-105')
  assert.equal(statement.label, 'Invalid XCSS statement')
  assert.equal(statement.legacyCode, 'syntax.invalid-statement')
  assert.equal(statement.message, 'Expected a declaration or rule')
  assert.deepEqual(statement.location, {
    uri,
    range: {
      start: { line: 0, character: source.indexOf('bogus') },
      end: { line: 0, character: source.indexOf(';') },
    },
  })
  for (const [code, legacyCode] of [
    ['XC-212', 'binding.unknown-injection'],
    ['XC-311', 'type.unknown-type'],
    ['XC-403', 'variants.unknown-class'],
  ]) {
    assert.equal(broken.find(diagnostic => diagnostic.code === code)?.legacyCode, legacyCode)
  }
  service.setTypescriptFile('tsconfig.json', 2, '{"files":["index.ts"]}')
  assert.equal(service.typescriptProjectWarning(), null)
  assert.deepEqual(service.document(uri).diagnostics, broken)
  service.setTypescriptFile('tsconfig.json', 3, '{ invalid:')
  service.setTypescriptProject(null)
  assert.equal(service.typescriptProjectWarning(), null)
  assert.deepEqual(service.document(uri).diagnostics, broken)
})

test('editor API returns UTF-16 facts without invoking compilation', () => {
  const service = new LanguageService()
  assert.equal(languageServiceCapabilities().apiVersion, 5)
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
