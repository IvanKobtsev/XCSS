// Run in a clean directory with @cross-css/native installed from the registry.
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { resolve } from 'node:path'
const require = createRequire(resolve(process.cwd(), 'package.json'))
const { LanguageService, languageServiceCapabilities } = require('@cross-css/native')
assert.equal(require('@cross-css/native/package.json').version, '0.3.0')
assert.equal(languageServiceCapabilities().apiVersion, 1)
const service = new LanguageService()
const source = '@injection paint {} .a { @:paint; }'
service.setFile('file:///test.xcss', 1, source)
assert.deepEqual(service.document('file:///test.xcss').diagnostics, [])
assert(service.document('file:///test.xcss').tokens.length > 0)
assert.equal(service.definition('file:///test.xcss', { line: 0, character: source.lastIndexOf('paint') }).length, 1)
console.log('Published XCSS language-service API verified')
