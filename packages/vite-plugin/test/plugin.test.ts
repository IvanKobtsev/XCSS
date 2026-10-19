import { describe, expect, it, vi } from 'vitest'

import xcss from '../src/index.js'
import { createVirtualId, parseVirtualId } from '../src/internal.js'

describe('virtual XCSS ids', () => {
  it('maps CSS and preserves Vite queries', () => {
    const virtual = createVirtualId('C:\\project\\button.cross.css?inline')
    expect(virtual?.id).toContain('C:/project/button.module.css?')
    expect(virtual?.id).toContain('inline=')
    expect(parseVirtualId(virtual!.id)).toMatchObject({
      sourcePath: 'C:/project/button.cross.css',
      syntax: 'css',
    })
  })

  it('maps SCSS to a CSS Module preprocessor id', () => {
    expect(createVirtualId('/project/theme.cross.scss')?.id).toMatch(
      /\/project\/theme\.module\.scss\?/,
    )
  })
})

describe('plugin shape', () => {
  it('runs before Vite core plugins', () => {
    const plugin = xcss()
    expect(plugin.name).toBe('cross-css')
    expect(plugin.enforce).toBe('pre')
    expect(plugin.resolveId).toBeTypeOf('function')
    expect(plugin.load).toBeTypeOf('function')
    expect(plugin.handleHotUpdate).toBeTypeOf('function')
  })

  it('delegates alias resolution and invalidates the virtual module on change', async () => {
    const plugin = xcss()
    const resolvedSource = 'C:/project/src/button.cross.css'
    const resolve = vi.fn().mockResolvedValue({ id: resolvedSource })
    const virtualId = await (plugin.resolveId as Function).call(
      { resolve },
      '@/button.cross.css?inline',
      'C:/project/src/main.ts',
      {},
    )

    expect(resolve).toHaveBeenCalledWith(
      '@/button.cross.css?inline',
      'C:/project/src/main.ts',
      { skipSelf: true },
    )
    expect(virtualId).toContain('button.module.css?')

    const virtualModule = { id: virtualId }
    const invalidateModule = vi.fn()
    const affected = (plugin.handleHotUpdate as Function)({
      file: resolvedSource.replaceAll('/', '\\'),
      modules: [],
      server: {
        moduleGraph: {
          getModuleById: vi.fn().mockReturnValue(virtualModule),
          invalidateModule,
        },
      },
    })

    expect(invalidateModule).toHaveBeenCalledWith(virtualModule)
    expect(affected).toContain(virtualModule)
  })
})
