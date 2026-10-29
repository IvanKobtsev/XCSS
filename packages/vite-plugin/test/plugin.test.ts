import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { normalizePath } from 'vite'
import { describe, expect, it, vi } from 'vitest'
import xcss from '../src/index.js'
import { createVirtualId, createCssId, parseVirtualId } from '../src/internal.js'
describe('virtual XCSS ids', () => {
 it('uses JavaScript wrappers and plain CSS without double module scoping',()=>{
  const virtual=createVirtualId('C:/project/button.xcss?inline')!;expect(virtual.id).toContain('button.xcss.js?');expect(virtual.mode).toBe('inline');expect(parseVirtualId(virtual.id)?.sourcePath).toBe('C:/project/button.xcss');expect(createCssId(virtual.sourcePath)).toContain('button.xcss.css?');expect(createCssId(virtual.sourcePath)).not.toContain('.module.css')
 })
 it('rejects unsupported and conflicting queries',()=>{expect(()=>createVirtualId('/a.xcss?raw')).toThrow(/Unsupported/);expect(()=>createVirtualId('/a.xcss?inline&url')).toThrow(/combine/);expect(createVirtualId('/a.scss')).toBeNull()})
})
describe('plugin shape',()=>{
 it('delegates alias resolution and claims generated modules',async()=>{
  const plugin=xcss();const resolve=vi.fn().mockResolvedValue({id:'C:/project/button.xcss'});const virtual=await (plugin.resolveId as Function).call({resolve},'@/button.xcss','main.ts',{});expect(virtual).toContain('button.xcss.js?');expect(resolve).toHaveBeenCalled();expect(await (plugin.resolveId as Function).call({resolve},virtual)).toBe(virtual)
 })
 it('invalidates active wrappers after edits',async()=>{
  const plugin=xcss();const virtual=await (plugin.resolveId as Function).call({resolve:async()=>({id:'C:/project/a.xcss'})},'./a.xcss',undefined,{});const module={id:virtual};const invalidateModule=vi.fn();const result=(plugin.handleHotUpdate as Function)({file:'C:/project/a.xcss',modules:[],server:{moduleGraph:{getModuleById:()=>module,invalidateModule}}});expect(invalidateModule).toHaveBeenCalledWith(module);expect(result).toContain(module)
 })
 it('anchors source maps to original files rather than the virtual CSS directory',async()=>{
  const root=fileURLToPath(new URL('../../../fixtures/vite-basic/',import.meta.url))
  const source=normalizePath(join(root,'src/styles.xcss'));const plugin=xcss()
  ;(plugin.configResolved as Function)({root,command:'serve',base:'/'})
  const result=await (plugin.load as Function).call({resolve:async(specifier:string,importer:string)=>({id:specifier.startsWith('@/')?join(root,'src',specifier.slice(2)):join(dirname(importer),specifier)}),addWatchFile:()=>{},warn:()=>{},error:(error:{message:string})=>{throw new Error(error.message)}},createCssId(source))
  expect(result.map.sources).toContain(source)
  expect(result.map.sources.some((path:string)=>path.includes('/src/src/'))).toBe(false)
 })

})
