import assert from 'node:assert/strict'
import test from 'node:test'
import { Compiler, inspect } from '../index.js'
function compile(source) { const compiler=new Compiler(); compiler.setFile('a.xcss',1,source); return compiler.compile('a.xcss') }

test('shares composed utilities across entries and updates the union after removal', () => {
 const c = new Compiler()
 c.setFile('util.xcss', 1, '@util trim($n: integer = 3) { max-height: $n * 1lh; }')
 c.setFile('a.xcss', 1, '.a { @:trim; }')
 c.setFile('b.xcss', 1, '.b { @:trim(3); } span { @:trim(5); }')
 const a = c.compile('a.xcss')
 assert.equal(a.success, true)
 assert.match(a.artifacts[0].classes.a, /_util_trim_3/)
 assert.equal(a.utilities.rules.length, 1)
 const union = c.compileUtilities(['a.xcss', 'b.xcss'])
 assert.equal(union.success, true)
 assert.equal(union.utilities.rules.length, 1)
 assert.match(union.utilities.css, /_util_trim_3/)
 assert.doesNotMatch(union.utilities.css, /_util_inj_|5lh/)
 assert(JSON.parse(union.utilities.sourceMap).sources.includes('util.xcss'))
 const bundled = c.compile('a.xcss', false, true)
 assert.equal(bundled.utilities, undefined)
 assert.match(bundled.artifacts[0].css, /_util_trim_3/)
 c.setFile('a.xcss', 2, '.a { color: red; }')
 c.setFile('b.xcss', 2, '.b { color: blue; }')
 assert.equal(c.compileUtilities(['a.xcss', 'b.xcss']).utilities, undefined)
})

test('substitutes immutable typed constants and retains runtime CSS math', () => {
 const result = compile('$base = 56px; $height: length = $base + --my-height / 2; .a { height: $height; }')
 assert.equal(result.success, true)
 assert.match(result.artifacts[0].css, /height: calc\(56px \+ var\(--my-height\) \/ 2\)/)
 assert.doesNotMatch(result.artifacts[0].css, /\$|@property/)
 const invalid = compile('$base = 56px; .a { $base = 8px; height: $base; }')
 assert.equal(invalid.success, false)
 assert.deepEqual(invalid.artifacts, [])
 assert(invalid.diagnostics.some(d => d.code === 'binding.constant-redeclaration'))
})
test('returns scoped CSS, exports and original source maps',()=>{
 const result=compile('.a { width: --size * 2; }'); assert.equal(result.success,true); const artifact=result.artifacts[0];
 assert.match(artifact.css,/calc\(var\(--size\) \* 2\)/); assert.ok(artifact.classes.a.startsWith('xcss_')); assert.ok(JSON.parse(artifact.sourceMap).sources.includes('a.xcss')); assert.deepEqual(result.diagnostics,[])
})
test('returns structured diagnostics without artifacts',()=>{
 const result=compile('@use "./tokens";'); assert.equal(result.success,false); assert.deepEqual(result.artifacts,[]); assert.equal(result.diagnostics[0].moduleId,'a.xcss'); assert.equal(result.diagnostics[0].line,1); assert.equal(result.diagnostics[0].code,'syntax.sass')
})
test('expands typed injections and content directly into CSS',()=>{
 const result=compile('@injection panel($size: length = 1px) { border-width: $size; @content; } .panel { @:panel(2px) { color: red; } }'); assert.equal(result.success,true); assert.match(result.artifacts[0].css,/border-width: 2px/); assert.doesNotMatch(result.artifacts[0].css,/@injection|@mixin|@include|\$size/)
})
test('inspects injection imports',()=>{ assert.deepEqual(inspect('@import ns from "./tokens.xcss"; .a { composes: b from "./b.xcss"; }').imports,['./tokens.xcss']) })
test('retains project state, updates global injection resolution and rejects stale revisions',()=>{
 const c=new Compiler();c.setFile('a.xcss',1,'@injection tone {}');c.setFile('b.xcss',2,'.b { @:tone; }');assert.equal(c.compile('b.xcss').success,true);c.setFile('c.xcss',3,'@injection tone {}');assert.equal(c.compile('b.xcss').success,false);c.removeFile('c.xcss',4);assert.equal(c.compile('b.xcss').success,true);assert.throws(()=>c.setFile('b.xcss',1,''),/StaleRevision/)
})

test('class dictionaries retain JavaScript prototype-shaped names as data',()=>{const result=compile('.__proto__ { color: red; } .constructor { color: blue; }');const classes=result.artifacts[0].classes;assert.equal(Object.hasOwn(classes,'__proto__'),true);assert.equal(typeof classes.__proto__,'string');assert.equal(typeof classes.constructor,'string')})


test('infers zero as length and compiles silent line comments', () => {
 const c = new Compiler()
 c.setFile('a.xcss', 1, '// 😀 lead\n@vars { --gap = 0; --count: integer = 0; }\n@injection f($n: number = 0) { opacity: $n; }\n.a // selector\n{ @:f(0); margin: --gap; width: calc(1px + // inline\n2px); }')
 for (const minify of [false, true]) {
  const result = c.compile('a.xcss', minify)
  assert.equal(result.success, true, JSON.stringify(result.diagnostics))
  assert.match(result.artifacts[0].css, /<length>/)
  assert.doesNotMatch(result.artifacts[0].css, /\/\//)
 }
})
