import { fileURLToPath } from 'node:url'

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
    expect(js).toContain('calc(var(--space) * 2)')
  })

  it('reports reserved imports before downstream CSS processing', async () => {
    const server = await createServer({
      root,
      configFile: false,
      logLevel: 'silent',
      plugins: [xcss()],
      server: { middlewareMode: true },
    })
    try {
      const request = server.transformRequest('/src/invalid.cross.scss')
      await expect(request).rejects.toMatchObject({
        message: expect.stringMatching(/XCSS import system/),
        id: expect.stringContaining('invalid.cross.scss'),
        loc: expect.objectContaining({ line: 1, column: 0 }),
      })
    } finally {
      await server.close()
    }
  })
})
