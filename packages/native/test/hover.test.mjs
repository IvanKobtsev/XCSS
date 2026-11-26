import assert from 'node:assert/strict'
import { test } from 'node:test'
import { LanguageService } from '../index.js'

test('hover returns semantic data across files with original UTF-16 ranges', () => {
  const service = new LanguageService()
  const uri = 'file:///app.xcss'
  const lib = 'file:///lib.xcss'
  const declaration = '@injection paint($p: percentage | number = 50%, $color: color) { @content; }'
  const source = '/* 😀 */ @import lib from "./lib"; .a { @:lib.paint(1, red) {} }'
  service.setFile(lib, 1, declaration)
  service.setFile(uri, 1, source)
  service.setResolution(uri, './lib', lib)
  const position = { line: 0, character: source.indexOf('paint') }
  const hover = service.hover(uri, position)
  assert.deepEqual(hover, {
    range: {
      start: { line: 0, character: source.indexOf('lib.paint') },
      end: { line: 0, character: source.indexOf('lib.paint') + 'lib.paint'.length },
    },
    kind: 'injection',
    name: 'paint',
    declaration: service.definition(uri, position)[0],
    valueType: { kind: 'unknown' },
    signature: {
      parameters: [
        { name: '$p', valueType: { kind: 'union', members: [
          { kind: 'named', name: 'percentage' }, { kind: 'named', name: 'number' },
        ] }, defaultValue: '50%' },
        { name: '$color', valueType: { kind: 'named', name: 'color' } },
      ],
      acceptsContent: true,
    },
  })
  assert.equal('text' in hover, false)
  service.removeFile(lib, 2)
  assert.equal(service.hover(uri, position), null)
  assert.equal(hover.signature.parameters[0].defaultValue, '50%')
})

test('hover preserves source values and separates return types and registrations', () => {
  const service = new LanguageService()
  const uri = 'a.xcss'
  const source = '$size = 2px + 3px; @vars { local --size: length = 1px; --theme: color; } @function size(): length => 2px; .a { width: $size; height: --size; color: --theme; margin: @:size(); }'
  service.setFile(uri, 1, source)
  const hover = needle => service.hover(uri, { line: 0, character: source.lastIndexOf(needle) })
  const constant = hover('$size')
  assert.equal(constant.kind, 'constant')
  assert.equal(constant.name, '$size')
  assert.equal(constant.constantValue, '2px + 3px')
  assert.equal('signature' in constant, false)
  assert.equal('variable' in constant, false)
  assert.deepEqual(hover('--size').variable, { inherits: false, initialValue: '1px' })
  assert.deepEqual(hover('--theme').variable, { inherits: true })
  const fn = hover('size()')
  assert.equal(fn.kind, 'function')
  assert.deepEqual(fn.valueType, { kind: 'named', name: 'length' })
  assert.deepEqual(fn.signature, { parameters: [], acceptsContent: false })
  service.setFile(uri, 2, source.replace('2px + 3px', '4px + 5px'))
  assert.equal(hover('$size').constantValue, '4px + 5px')
  assert.equal(constant.constantValue, '2px + 3px')
})
