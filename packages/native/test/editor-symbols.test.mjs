import { test } from 'node:test'
import assert from 'node:assert/strict'
import { LanguageService, languageServiceCapabilities } from '../index.js'

test('class, group and member locations and completion cross the native boundary', () => {
  assert.equal(languageServiceCapabilities().apiVersion, 5)
  const service = new LanguageService()
  const css = '.primary {} .secondary {}\n@variants Button: .primary, .secondary;'
  const ts = 'import styles, { type Variants as V } from "./theme.xcss";\n/* 😀 */ styles.primary; const value: V<"Button"> = "primary";'
  service.setFile('theme.xcss', 1, css)
  service.setTypescriptProject('tsconfig.json')
  service.setTypescriptFile('tsconfig.json', 1, '{"files":["app.ts"]}')
  service.setTypescriptFile('app.ts', 1, ts)
  const character = ts.split('\n')[1].indexOf('"primary"') + 1
  const position = { line: 1, character }
  const member = { line: 1, character: 19 }
  assert.deepEqual(service.definition('app.ts', position)[0].range.start, member)
  const symbols = service.symbolsAt('theme.xcss', member)
  assert.deepEqual(symbols, [{ module: 'theme.xcss', kind: 'variantMember', group: 'Button', name: 'primary' }])
  assert.deepEqual(service.referencesForSymbols(symbols, false)[0].range.start, position)
  const secondary = service.completion('app.ts', position).find(item => item.label === 'secondary')
  assert.deepEqual(secondary, {
    label: 'secondary', kind: 'variantMember', newText: 'secondary',
    range: { start: position, end: { line: 1, character: character + 'primary'.length } },
  })
})
