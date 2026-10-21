import { mkdir, readFile, readdir, rename, rm, writeFile } from 'node:fs/promises'
import { dirname, join, relative } from 'node:path'

import { inspect, transform } from '@cross-css/native'
import type { ModuleNode, Plugin } from 'vite'

import { createVirtualId, isCrossCssId, parseVirtualId } from './internal.js'

let partialSequence = 0
let partialWrite = Promise.resolve()

export default function xcss(): Plugin {
  const virtualIdsBySource = new Map<string, Set<string>>()
  let projectRoot = process.cwd()
  let rebuild: Promise<ProjectRegistry> | undefined

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

      const registry = await (rebuild ??= buildRegistry(projectRoot, async (specifier, importer) => {
        const resolved = await this.resolve(specifier, importer, { skipSelf: true })
        return resolved && !resolved.external ? resolved.id : null
      }).finally(() => { rebuild = undefined }))
      for (const file of registry.files) this.addWatchFile(file.filename)
      const moduleId = toModuleId(projectRoot, virtual.sourcePath)
      const result = transform(registry.files, registry.resolutions, moduleId, registry.partialPath)
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
      if (result.partial === null) this.error('XCSS did not generate an injection registry')
      await writePartial(registry.partialPath, result.partial)
      return { code: result.code, map: null }
    },

    handleHotUpdate(context) {
      if (!context.file.toLowerCase().endsWith('.xcss')) return
      rebuild = undefined

      const affected = new Set<ModuleNode>(context.modules)
      for (const ids of virtualIdsBySource.values()) {
        for (const id of ids) {
          const module = context.server.moduleGraph.getModuleById(id)
          if (module) {
            context.server.moduleGraph.invalidateModule(module)
            affected.add(module)
          }
        }
      }
      return [...affected]
    },
  }
}

interface ProjectRegistry {
  files: Array<{ moduleId: string; filename: string; source: string }>
  resolutions: Array<{ importer: string; specifier: string; target: string }>
  partialPath: string
}

async function buildRegistry(
  root: string,
  resolve: (specifier: string, importer: string) => Promise<string | null>,
): Promise<ProjectRegistry> {
  const filenames = await findXcssFiles(root)
  const files = await Promise.all(filenames.map(async (filename) => ({
    moduleId: toModuleId(root, filename),
    filename,
    source: await readFile(filename, 'utf8'),
  })))
  const byFilename = new Map(files.map((file) => [normalize(file.filename), file.moduleId]))
  const resolutions: ProjectRegistry['resolutions'] = []
  for (const file of files) {
    for (const specifier of inspect(file.source).imports) {
      const resolved = await resolve(specifier, file.filename)
      resolutions.push({
        importer: file.moduleId,
        specifier,
        target: resolved ? byFilename.get(normalize(sourcePathFromResolved(resolved))) ?? '' : '',
      })
    }
  }
  return {
    files,
    resolutions,
    partialPath: normalize(join(root, 'node_modules', '.xcss', '_injections.scss')),
  }
}

async function findXcssFiles(root: string): Promise<string[]> {
  const output: string[] = []
  async function visit(directory: string): Promise<void> {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      if (entry.isDirectory() && ['node_modules', '.git', 'dist', 'target', 'coverage'].includes(entry.name)) continue
      const path = join(directory, entry.name)
      if (entry.isDirectory()) await visit(path)
      else if (entry.isFile() && entry.name.toLowerCase().endsWith('.xcss')) output.push(normalize(path))
    }
  }
  await visit(root)
  return output.sort()
}

async function writePartial(path: string, contents: string): Promise<void> {
  partialWrite = partialWrite.then(() => writePartialNow(path, contents))
  return partialWrite
}

async function writePartialNow(path: string, contents: string): Promise<void> {
  await mkdir(dirname(path), { recursive: true })
  try {
    if (await readFile(path, 'utf8') === contents) return
  } catch {}
  const temporary = `${path}.${process.pid}.${partialSequence++}.tmp`
  await writeFile(temporary, contents)
  await rm(path, { force: true })
  await rename(temporary, path)
}

function toModuleId(root: string, path: string): string {
  return normalize(relative(root, path))
}

function normalize(path: string): string {
  return path.replaceAll('\\', '/')
}

function stripQuery(id: string): string {
  return id.split('?', 1)[0]
}

function sourcePathFromResolved(id: string): string {
  const queryStart = id.indexOf('?')
  if (queryStart !== -1) {
    const source = new URLSearchParams(id.slice(queryStart + 1)).get('xcss-source')
    if (source) return source
  }
  return stripQuery(id)
}

export { xcss }
