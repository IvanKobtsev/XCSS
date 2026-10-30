import { test } from 'node:test'
import assert from 'node:assert/strict'
import { LanguageService, languageServiceCapabilities } from '../index.js'

function analyze(source) {
  const service = new LanguageService()
  service.setFile('file:///test.xcss', 1, source)
  return { service, result: service.cssDeclarations('file:///test.xcss') }
}

test('CSS declarations preserve literals, source order and UTF-16 locations', () => {
  assert.equal(languageServiceCapabilities().apiVersion, 2)
  const source = '/* 😀 */ .a { dispay: flex;\r\n  display: fex; &:hover { opacity: 1; } } @keyframes x { to { width: 1px; } }'
  const { result } = analyze(source)
  assert.deepEqual(result.declarations.map(d => d.property), ['dispay', 'display', 'opacity', 'width'])
  assert.equal(result.declarations[0].propertyRange.start.character, source.indexOf('dispay'))
  assert.deepEqual(result.declarations[1].propertyRange.start, { line: 1, character: 2 })
  assert(result.declarations.every(d => d.checkable))
  assert.equal(result.declarations[1].parts.map(p => p.text ?? '').join(''), ' fex')
})

test('typed references use declared types and arithmetic uses analysis facts', () => {
  const { result } = analyze('@vars { --t: time = 1s; --n: number = 1; } @injection paint($p: length = 1px) { width: $p; border: 1px solid red; } .a { width: --t; z-index: --n; animation-duration: --t + --t; color: rgb(1 2 3 / --n); }')
  const declarations = result.declarations
  assert.deepEqual(declarations[0].parts.filter(p => p.valueType).map(p => p.valueType), ['length'])
  assert.deepEqual(declarations[2].parts.filter(p => p.valueType).map(p => p.valueType), ['time'])
  assert.deepEqual(declarations[4].parts.map(p => p.valueType), ['time'])
  assert(declarations[4].checkable)
  assert(declarations[5].checkable)
})

test('directives and descriptors are excluded; unprojectable values retain properties', () => {
  const { result } = analyze('@vars { --t: time; } @font-face { font-family: x; src: url(x); } @property --x { syntax: "<color>"; inherits: false; initial-value: red; } .a { composes: b; content: /* hi */ preserve; width: --unknown; height: var(--x); transition: (opacity, color) 1s; color: red; }')
  assert.deepEqual(result.declarations.map(d => d.property), ['width', 'height', 'transition', 'color'])
  assert.deepEqual(result.declarations.map(d => d.checkable), [false, false, false, true])
})

test('dependency changes and unsaved revisions update declaration types', () => {
  const { service } = analyze('@import lib from "./lib"; .a { @:lib.paint; }')
  service.setFile('file:///lib.xcss', 1, '@injection paint($p: time = 1s) { width: $p; }')
  service.setResolution('file:///test.xcss', './lib', 'file:///lib.xcss')
  const old = service.cssDeclarations('file:///lib.xcss')
  assert.equal(old.declarations[0].parts.find(p => p.valueType).valueType, 'time')
  service.setFile('file:///lib.xcss', 2, '@injection paint($p: length = 1px) { width: $p; }')
  const updated = service.cssDeclarations('file:///lib.xcss')
  assert(updated.revision > old.revision)
  assert.equal(updated.declarations[0].parts.find(p => p.valueType).valueType, 'length')
  assert.equal(old.declarations[0].parts.find(p => p.valueType).valueType, 'time')
})

test('recovery keeps property checks but disables value checks', () => {
  const { result } = analyze('.a { dispay: $missing; width: calc(1px;')
  assert(result.declarations.length > 0)
  assert(result.declarations.every(d => !d.checkable))
})
