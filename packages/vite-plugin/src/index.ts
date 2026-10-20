import { readFile } from 'node:fs/promises'
import { relative } from 'node:path'

import { transform } from '@cross-css/native'
import type { ModuleNode, Plugin } from 'vite'

import { createVirtualId, isCrossCssId, parseVirtualId } from './internal.js'

export default function xcss(): Plugin {
  const virtualIdsBySource = new Map<string, Set<string>>()
  let projectRoot = process.cwd()

  return {
    name: 'cross-css',
    enforce: 'pre',

    configResolved(config) {
      projectRoot = config.root
    },

    async resolveId(source, importer, options) {
      if (!isCrossCssId(source)) return null
      const resolved = await this.resolve(source, importer, {
        ...options,
        skipSelf: true,
      })
      if (!resolved || resolved.external) return null
      const virtual = createVirtualId(resolved.id)
      if (!virtual) return null

      let ids = virtualIdsBySource.get(virtual.sourcePath)
      if (!ids) {
        ids = new Set()
        virtualIdsBySource.set(virtual.sourcePath, ids)
      }
      ids.add(virtual.id)
      return virtual.id
    },

    async load(id) {
      const virtual = parseVirtualId(id)
      if (!virtual) return null

      this.addWatchFile(virtual.sourcePath)
      const source = await readFile(virtual.sourcePath, 'utf8')
      const moduleId = relative(projectRoot, virtual.sourcePath).replaceAll('\\', '/')
      const result = transform(source, virtual.syntax, virtual.sourcePath, moduleId)
      if (result.code === null) {
        const [first, ...rest] = result.diagnostics
        const details = rest.map((diagnostic) => diagnostic.message).join('\n')
        this.error({
          id: virtual.sourcePath,
          message: first
            ? `${first.message}${details ? `\n${details}` : ''}`
            : 'XCSS compilation failed',
          loc: first
            ? {
                file: first.filename ?? virtual.sourcePath,
                line: first.line,
                column: Math.max(0, first.column - 1),
              }
            : undefined,
        })
      }
      return { code: result.code, map: null }
    },

    handleHotUpdate(context) {
      const ids = virtualIdsBySource.get(context.file.replaceAll('\\', '/'))
      if (!ids) return

      const affected = new Set<ModuleNode>(context.modules)
      for (const id of ids) {
        const module = context.server.moduleGraph.getModuleById(id)
        if (module) {
          context.server.moduleGraph.invalidateModule(module)
          affected.add(module)
        }
      }
      return [...affected]
    },
  }
}

export { xcss }
