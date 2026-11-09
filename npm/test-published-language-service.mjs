// Run in a clean directory with @cross-css/native installed from the registry.
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { resolve } from 'node:path'
import { readFileSync } from 'node:fs'
const require = createRequire(resolve(process.cwd(), 'package.json'))
const { LanguageService, languageServiceCapabilities } = require('@cross-css/native')
const expectedVersion = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8')).version
assert.equal(require('@cross-css/native/package.json').version, expectedVersion)
assert.equal(languageServiceCapabilities().apiVersion, 4)
const service = new LanguageService()
const source = '@injection paint {} .a { @:paint; }'
service.setFile('file:///test.xcss', 1, source)
assert.deepEqual(service.document('file:///test.xcss').diagnostics, [])
assert(service.document('file:///test.xcss').tokens.length > 0)
assert.equal(service.definition('file:///test.xcss', { line: 0, character: source.lastIndexOf('paint') }).length, 1)


assert.equal(service.cssDeclarations('file:///test.xcss').declarations.length, 0)
service.setFile('file:///css.xcss', 1, '.a { display: flex; }')
assert.equal(service.cssDeclarations('file:///css.xcss').declarations[0].property, 'display')
assert.equal(service.cssValues('file:///css.xcss').values[0].property, 'display')
assert.deepEqual(languageServiceCapabilities().tokenTypes.slice(-3), ['cssFunction', 'cssKeyword', 'cssColor'])

service.setFile('file:///selectors.xcss', 1, '#root, .parent { a.button[data-kind="hello"]:where(.disabled)::after { & > * {} } }')
const selectors = service.selectors('file:///selectors.xcss')
assert.equal(selectors.selectors.length, 4)
assert.deepEqual(selectors.selectors[2].specificity, { ids: 1, classes: 2, types: 2 })
const legend = languageServiceCapabilities().tokenTypes
const tokens = service.document('file:///selectors.xcss').tokens
for (const kind of ['class', 'selectorId', 'selectorTag', 'selectorAttribute', 'pseudoClass', 'pseudoElement', 'selectorUniversal', 'selectorNesting', 'selectorCombinator']) {
  assert(tokens.some(token => legend[token.tokenType] === kind), kind)
}
const tsService = new LanguageService()
tsService.setFile('button.xcss', 1, '@variants Button: .primary; .button {} .primary {}')
tsService.setTypescriptProject('tsconfig.json')
tsService.setTypescriptFile('tsconfig.json', 1, JSON.stringify({ compilerOptions: { strict: true }, files: ['index.ts'] }))
tsService.setTypescriptFile('index.ts', 1, "import styles, { type Variants } from './button.xcss'; const value: Variants<'Button'> = 'wrong'; styles.missing;")
assert.deepEqual(tsService.document('index.ts').diagnostics.map(({ code }) => code), ['XC-003', 'XC-001'])
tsService.setTypescriptFile('index.ts', 2, "import styles, { type Variants } from './button.xcss'; const value: Variants<'Button'> = 'primary'; styles.button;")
assert.deepEqual(tsService.document('index.ts').diagnostics, [])
tsService.setTypescriptFile('tsconfig.json', 2, '{ invalid:')
tsService.setFile('broken.xcss', 1, 'bogus;')
assert.match(tsService.typescriptProjectWarning(), /Couldn't analyze XCSS TypeScript integration/)
const [stylesheetDiagnostic] = tsService.document('broken.xcss').diagnostics
assert.equal(stylesheetDiagnostic.code, 'XC-105')
assert.equal(stylesheetDiagnostic.label, 'Invalid XCSS statement')
assert.equal(stylesheetDiagnostic.legacyCode, 'syntax.invalid-statement')
console.log('Published XCSS language-service API v4, selectors, and TypeScript variants verified')
