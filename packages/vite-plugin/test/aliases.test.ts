import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { build, createServer, normalizePath } from 'vite'
import { describe, expect, it } from 'vitest'
import xcss from '../src/index.js'
import { createCssId } from '../src/internal.js'

async function fixture() {
  const parent = fileURLToPath(new URL('../../../target/vite-alias-tests/', import.meta.url))
  await mkdir(parent, { recursive: true })
  const root = await mkdtemp(join(parent, 'project-'))
  await mkdir(join(root, 'src/deep'), { recursive: true })
  await mkdir(join(root, 'assets'), { recursive: true })
  await writeFile(join(root, 'assets/icon.svg'), '<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0h8v8z"/></svg>')
  await writeFile(join(root, 'base.json'), JSON.stringify({ compilerOptions: { paths: { '@styles/*': ['./src/*'], '@assets/*': ['./assets/*'] } } }))
  await writeFile(join(root, 'tsconfig.json'), '{"extends":"./base.json"}')
  await writeFile(join(root, 'src/recipes.xcss'), '@injection picture { background: url("@assets/icon.svg?q=1#part"); } @util masked { mask: url(@assets/icon.svg); }')
  await writeFile(join(root, 'src/deep/main.xcss'), '@import { picture, masked } from "@styles/recipes.xcss"; .a { @:masked; @:picture; }')
  await writeFile(join(root, 'entry.js'), 'import { styles } from "./src/deep/main.xcss"; console.log(styles.a);')
  return root
}

describe('tsconfig stylesheet and URL aliases', () => {
  it('uses tsconfig for recipes and assets in production despite conflicting Vite aliases', async () => {
    const root = await fixture()
    const result = await build({ root, configFile: false, logLevel: 'silent', plugins: [xcss()], resolve: { alias: { '@styles': '/wrong', '@assets': '/wrong' } }, build: { write: false, assetsInlineLimit: 0, rollupOptions: { input: join(root, 'entry.js') } } })
    const built = Array.isArray(result) ? result[0] : result
    if (!('output' in built)) throw new Error('Unexpected watch build')
    const output = built.output
    const css = output.flatMap(item => item.type === 'asset' && item.fileName.endsWith('.css') ? [String(item.source)] : []).join('\n')
    expect(css).toContain('?q=1#part')
    expect(css).toContain('mask:')
    expect(css).not.toContain('@assets')
    expect(css).not.toContain('/wrong')
    expect(output.some(item => item.fileName.endsWith('.svg'))).toBe(true)
  })
  it('serves resolved URLs in dev and refreshes extended config mappings', async () => {
    const root = await fixture()
    const server = await createServer({ root, configFile: false, logLevel: 'silent', plugins: [xcss()], server: { middlewareMode: true } })
    try {
      const id = createCssId(normalizePath(join(root, 'src/deep/main.xcss')))
      const loaded = await server.pluginContainer.load(id)
      const code = typeof loaded === 'string' ? loaded : loaded?.code
      expect(code).toContain('../../assets/icon.svg?q=1#part')
      await mkdir(join(root, 'other'))
      await writeFile(join(root, 'other/icon.svg'), await readFile(join(root, 'assets/icon.svg')))
      await writeFile(join(root, 'base.json'), JSON.stringify({ compilerOptions: { paths: { '@styles/*': ['./src/*'], '@assets/*': ['./other/*'] } } }))
      const plugin = server.config.plugins.find(plugin => plugin.name === 'cross-css')!
      ;(plugin.handleHotUpdate as Function)({ file: normalizePath(join(root, 'base.json')), modules: [], server })
      const updated = await server.pluginContainer.load(id)
      expect(typeof updated === 'string' ? updated : updated?.code).toContain('../../other/icon.svg?q=1#part')
    } finally { await server.close() }
  })
  it('rejects aliases declared only in Vite and missing aliased URLs', async () => {
    const root = await fixture()
    const plugin = xcss()
    ;(plugin.configResolved as Function)({ root, command: 'serve', base: '/', build: { assetsDir: 'assets', outDir: 'dist' } })
    const context = { addWatchFile() {}, warn() {}, error(error: {message: string}) { throw new Error(error.message) }, resolve() { throw new Error('Vite resolver must not handle stylesheet edges') } }
    await writeFile(join(root, 'src/deep/main.xcss'), '@import { picture } from "@vite/recipes.xcss"; .a { @:picture; }')
    await expect((plugin.load as Function).call(context, createCssId(normalizePath(resolve(root, 'src/deep/main.xcss'))))).rejects.toThrow()
    await writeFile(join(root, 'src/deep/main.xcss'), '.a { background: url(@assets/missing.svg); }')
    ;(plugin.buildStart as Function).call({ addWatchFile() {} })
    await expect((plugin.load as Function).call(context, createCssId(normalizePath(resolve(root, 'src/deep/main.xcss'))))).rejects.toThrow(/Cannot resolve tsconfig paths alias/)
  })
})
