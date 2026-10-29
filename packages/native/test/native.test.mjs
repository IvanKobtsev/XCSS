import assert from 'node:assert/strict'
import test from 'node:test'
import { Compiler, inspect } from '../index.js'
function compile(source) { const compiler=new Compiler(); compiler.setFile('a.xcss',1,source); return compiler.compile('a.xcss') }
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
test('inspects both injection imports and compositions',()=>{ assert.deepEqual(inspect('@import ns from "./tokens.xcss"; .a { composes: b from "./b.xcss"; }').imports,['./b.xcss','./tokens.xcss']) })
test('retains project state, updates global injection resolution and rejects stale revisions',()=>{
 const c=new Compiler();c.setFile('a.xcss',1,'@injection tone {}');c.setFile('b.xcss',2,'.b { @:tone; }');assert.equal(c.compile('b.xcss').success,true);c.setFile('c.xcss',3,'@injection tone {}');assert.equal(c.compile('b.xcss').success,false);c.removeFile('c.xcss',4);assert.equal(c.compile('b.xcss').success,true);assert.throws(()=>c.setFile('b.xcss',1,''),/StaleRevision/)
})
test('returns composition artifacts and supports bundled inline output',()=>{
 const c=new Compiler();c.setFile('a.xcss',1,'.a { composes: b from "./b.xcss"; }');c.setFile('b.xcss',1,'.b { color: red; }');c.setResolution('a.xcss','./b.xcss','b.xcss');assert.equal(c.compile('a.xcss').artifacts.length,2);const bundle=c.compile('a.xcss',false,true);assert.equal(bundle.artifacts.length,1);assert.match(bundle.artifacts[0].css,/color: red/)
})

test('class dictionaries retain JavaScript prototype-shaped names as data',()=>{const result=compile('.__proto__ { color: red; } .constructor { color: blue; }');const classes=result.artifacts[0].classes;assert.equal(Object.hasOwn(classes,'__proto__'),true);assert.equal(typeof classes.__proto__,'string');assert.equal(typeof classes.constructor,'string')})
