import assert from 'node:assert/strict'
import { test } from 'node:test'
import { LanguageService, languageServiceCapabilities } from '../index.js'

test('selector API classifies original UTF-16 source and calculates effective specificity', () => {
  const service = new LanguageService()
  const uri = 'untitled:selectors.xcss'
  const text = '/* 😀 */ #parent, .parent { svg|a.café#id[data-x="hi"]:where(.off)::after > * { & + .child {} } }'
  service.setFile(uri, 1, text)
  const result = service.selectors(uri)
  assert.equal(languageServiceCapabilities().apiVersion, 3)
  assert.equal(result.selectors.length, 4)
  const rule = result.selectors[2]
  assert.deepEqual(rule.specificity, { ids: 2, classes: 2, types: 2 })
  const parts = rule.parts.map(p => [p.kind, text.slice(p.range.start.character, p.range.end.character)])
  assert(parts.some(([kind, value]) => kind === 'class' && value === 'café'))
  assert(parts.some(([kind, value]) => kind === 'selectorAttribute' && value === 'data-x'))
  assert(parts.some(([kind, value]) => kind === 'pseudoClass' && value === ':where'))
  const legend = languageServiceCapabilities().tokenTypes
  const tokens = service.document(uri).tokens
  for (const kind of ['class', 'selectorId', 'selectorTag', 'selectorAttribute', 'pseudoClass', 'pseudoElement', 'selectorUniversal', 'selectorNesting', 'selectorCombinator', 'selectorNamespace']) {
    assert(tokens.some(t => legend[t.tokenType] === kind), kind)
  }
  service.setFile(uri, 2, '.new {}')
  assert(service.selectors(uri).revision > result.revision)
  assert.equal(service.selectors(uri).selectors.length, 1)
})
