import { fileURLToPath } from 'node:url'
import { readFile } from 'node:fs/promises'
import { join } from 'node:path'

import { build, createServer, type Rollup } from 'vite'
import { describe, expect, it } from 'vitest'

import xcss from '@cross-css/vite-plugin'

const root = fileURLToPath(new URL('..', import.meta.url))

describe('Vite integration', () => {
  it('builds CSS, SCSS, modules, and inline imports through XCSS', async () => {
    const result = await build({
      root,
      configFile: false,
      logLevel: 'silent',
      plugins: [xcss()],
      resolve: { alias: { '@': join(root, 'src') } },
      build: { write: false },
    })
    const outputs = Array.isArray(result) ? result.flatMap((item) => item.output) : result.output
    const css = outputs
      .filter((item): item is Rollup.OutputAsset => item.type === 'asset' && item.fileName.endsWith('.css'))
      .map((item) => String(item.source))
      .join('\n')
    const js = outputs
      .filter((item): item is Rollup.OutputChunk => item.type === 'chunk')
      .map((item) => item.code)
      .join('\n')

    expect(css).toContain('calc(100% - var(--sidebar))')
    expect(css).toContain('calc(var(--unit) * 2)')
    expect(css).not.toContain('.card{')
    expect(css).toContain('border:2px solid')
    const scopedVars = [...css.matchAll(/--shared-tone-xcss-[0-9a-f]{16}/g)].map(
      ([name]) => name,
    )
    expect(new Set(scopedVars).size).toBe(2)
    expect(js).toContain('calc(var(--space) * 2)')
    const partial = await readFile(join(root, 'node_modules/.xcss/_injections.scss'), 'utf8')
    expect(partial).toContain('@mixin xcss-')
    expect(partial).not.toContain('.card')
  })

  it('reports reserved imports before downstream CSS processing', async () => {
    const server = await createServer({
      root,
      configFile: false,
      logLevel: 'silent',
      plugins: [xcss()],
      resolve: { alias: { '@': join(root, 'src') } },
      server: { middlewareMode: true },
    })
    try {
      const request = server.transformRequest('/src/invalid.xcss')
      await expect(request).rejects.toMatchObject({
        message: expect.stringMatching(/XCSS import system/),
        id: expect.stringContaining('invalid.xcss'),
        loc: expect.objectContaining({ line: 1, column: 0 }),
      })
    } finally {
      await server.close()
    }
  })
})
