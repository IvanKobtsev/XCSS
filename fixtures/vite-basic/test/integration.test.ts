import { fileURLToPath } from 'node:url'
import { join, relative, isAbsolute } from 'node:path'
import { mkdtemp, writeFile, rm } from 'node:fs/promises'
import { build, createServer, type Rollup } from 'vite'
import { describe, expect, it } from 'vitest'
import xcss from '@cross-css/vite-plugin'
const root=fileURLToPath(new URL('..',import.meta.url))
describe('Vite integration',()=>{
 it('builds scoped CSS, export dictionaries, inline text and stylesheet URLs without Sass',async()=>{
  const result=await build({root,configFile:false,logLevel:'silent',plugins:[xcss()],resolve:{alias:{'@':join(root,'src')}},build:{write:false,assetsInlineLimit:0,sourcemap:true}})
  const outputs=Array.isArray(result)?result.flatMap(item=>item.output):result.output
  const css=outputs.filter((item):item is Rollup.OutputAsset=>item.type==='asset'&&item.fileName.endsWith('.css')).map(item=>String(item.source)).join('\n')
  const js=outputs.filter((item):item is Rollup.OutputChunk=>item.type==='chunk').map(item=>item.code).join('\n')
  expect(css).toContain('var(--sidebar)');expect(css).toContain('var(--unit)');expect(css).toContain('border:2px solid');expect(css).toContain('xcss_');expect(css).not.toMatch(/@mixin|@include|@injection/);expect(css).not.toContain('.card{')
  expect(new Set([...css.matchAll(/--shared-tone-xcss-[0-9a-f]{16}/g)].map(([name])=>name)).size).toBe(2)
  expect(js).toContain('var(--space)');expect(outputs.some(item=>item.fileName.endsWith('.svg'))).toBe(true);expect(outputs.filter(item=>item.fileName.endsWith('.css')).length).toBeGreaterThanOrEqual(2)
 })
 it('reports original XCSS diagnostics before downstream processing',async()=>{
  const server=await createServer({root,configFile:false,logLevel:'silent',plugins:[xcss()],server:{middlewareMode:true}})
  try {await expect(server.transformRequest('/src/invalid.xcss')).rejects.toMatchObject({message:expect.stringMatching(/Sass syntax/),id:expect.stringContaining('invalid.xcss'),loc:expect.objectContaining({line:1,column:0})})}finally{await server.close()}
 })
 it('serves wrappers and processes inline CSS without injection',async()=>{
  const server=await createServer({root,configFile:false,logLevel:'silent',plugins:[xcss()],server:{middlewareMode:true}})
  try {const wrapper=await server.transformRequest('/src/styles.xcss');expect(wrapper?.code).toContain('export default');expect(wrapper?.code).toContain('xcss_');const inline=await server.transformRequest('/src/inline.xcss?inline');expect(inline?.code).not.toContain('updateStyle')}finally{await server.close()}
 })
 it('updates global injection resolution after edits, additions and removals',async()=>{
  const directory=await mkdtemp(join(root,'test-hmr-'))
  const injection='hmr-'+relative(root,directory).replace(/[^a-z0-9-]/gi,'-')
  const library=join(directory,'library.xcss');const consumer=join(directory,'consumer.xcss');const duplicate=join(directory,'duplicate.xcss')
  await writeFile(library,'@injection '+injection+' { width: 11px; }');await writeFile(consumer,'.consumer { @:'+injection+'; }')
  const server=await createServer({root,configFile:false,logLevel:'silent',plugins:[xcss()],server:{middlewareMode:true}})
  try {
   const wrapper=await server.transformRequest('/'+relative(root,consumer).replaceAll('\\','/'))
   const cssUrl=wrapper!.code.match(/import\s*["']([^"']+\.xcss\.css\?[^"']+)["']/)![1]
   expect((await server.transformRequest(cssUrl))?.code).toContain('11px')
   await writeFile(library,'@injection '+injection+' { width: 17px; }')
   await expect.poll(async()=> (await server.transformRequest(cssUrl))?.code.includes('17px'),{timeout:10000}).toBe(true)
   await writeFile(duplicate,'@injection '+injection+' { width: 22px; }')
   await expect.poll(async()=> {try{await server.transformRequest(cssUrl);return false}catch(error){return String((error as { message?: string }).message ?? error).includes('Ambiguous injection')}},{timeout:10000}).toBe(true)
   await rm(duplicate)
   await expect.poll(async()=> {try{return (await server.transformRequest(cssUrl))?.code.includes('17px')}catch{return false}},{timeout:10000}).toBe(true)
  } finally {
   await server.close()
   const within=relative(root,directory);if(within.startsWith('..')||isAbsolute(within))throw new Error('Invalid test cleanup directory')
   await rm(directory,{recursive:true,force:true})
  }
 },20000)

})
