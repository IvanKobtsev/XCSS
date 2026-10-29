import { readFile, readdir } from 'node:fs/promises'
import { join, relative, resolve as resolvePath } from 'node:path'
import { Compiler, inspect } from '@cross-css/native'
import type { NativeArtifact, NativeCompilation } from '@cross-css/native'
import { normalizePath, type Plugin, type ModuleNode, type ViteDevServer } from 'vite'
import { createCssId, createVirtualId, isCrossCssId, parseVirtualId } from './internal.js'

export default function xcss(): Plugin {
  const compiler = new Compiler()
  const sources = new Map<string, { source: string; revision: number }>()
  const activeIds = new Set<string>()
  let root = process.cwd()
  let build = false
  let base = '/'
  let revision = 0
  let projectVersion = 0
  let sync: Promise<void> | undefined
  const idFor = (path: string) => normalizePath(relative(root, path))
  const fileFor = (id: string) => normalizePath(resolvePath(root, id))
  const invalidate = (server: ViteDevServer) => {
    projectVersion++
    sync = undefined
    for (const id of activeIds) { const module = server.moduleGraph.getModuleById(id); if (module) server.moduleGraph.invalidateModule(module) }
  }
  async function synchronize(version: number, resolver: (specifier: string, importer: string) => Promise<string | null>) {
    const filenames = await findXcssFiles(root)
    const loaded = await Promise.all(filenames.map(async filename => ({ filename, source: await readFile(filename, 'utf8') })))
    const resolutions: Array<{ importer: string; specifier: string; target: string | null }> = []
    for (const file of loaded) for (const specifier of inspect(file.source).imports) {
      const resolved = await resolver(specifier, file.filename)
      resolutions.push({ importer: idFor(file.filename), specifier, target: resolved ? idFor(resolved.split('?')[0]) : null })
    }
    if (version !== projectVersion) return
    const current = new Set(loaded.map(file => idFor(file.filename)))
    for (const id of sources.keys()) if (!current.has(id)) { compiler.removeFile(id, ++revision); sources.delete(id) }
    for (const file of loaded) {
      const id = idFor(file.filename)
      if (sources.get(id)?.source !== file.source) { compiler.setFile(id, ++revision, file.source); sources.set(id, { source: file.source, revision }) }
    }
    for (const resolution of resolutions) compiler.setResolution(resolution.importer, resolution.specifier, resolution.target)
  }
  function resultFor(result: NativeCompilation, fail: (error: { message: string; id?: string; loc?: { file: string; line: number; column: number } }) => never): Map<string, NativeArtifact> {
    if (!result.success) {
      const diagnostic = result.diagnostics.find(d => d.severity === 'error')
      fail({ message: diagnostic?.message ?? 'XCSS compilation failed', id: diagnostic ? fileFor(diagnostic.moduleId) : undefined, loc: diagnostic ? { file: fileFor(diagnostic.moduleId), line: diagnostic.line, column: diagnostic.column - 1 } : undefined })
    }
    return new Map(result.artifacts.map(artifact => [artifact.moduleId, artifact]))
  }
  return {
    name: 'cross-css', enforce: 'pre',
    buildStart() { sync = undefined; projectVersion++; this.addWatchFile(root) },
    configResolved(config) { root = config.root; build = config.command === 'build'; base = config.base },
    configureServer(server) {
      const changed = (file: string) => { if (file.toLowerCase().endsWith('.xcss')) { invalidate(server); server.ws.send({ type: 'full-reload' }) } }
      const edited = (file: string) => { if (file.toLowerCase().endsWith('.xcss')) invalidate(server) }
      server.watcher.on('add', changed)
      server.watcher.on('unlink', changed)
      server.watcher.on('change', edited)
      server.httpServer?.once('close', () => { server.watcher.off('add', changed); server.watcher.off('unlink', changed); server.watcher.off('change', edited) })
    },
    async resolveId(source, importer, options) {
      if (parseVirtualId(source)) return source
      if (!isCrossCssId(source)) return null
      const resolved = await this.resolve(source, importer, { ...options, skipSelf: true })
      if (!resolved || resolved.external) return null
      const virtual = parseVirtualId(resolved.id) ?? createVirtualId(resolved.id)
      if (virtual) activeIds.add(virtual.id)
      return virtual?.id ?? null
    },
    async load(id) {
      const virtual = parseVirtualId(id)
      if (!virtual) return null
      // Vite's native CSS ?url loader emits and rewrites the stylesheet asset.
      if (virtual.kind !== 'module' && virtual.mode === 'url') return null
      activeIds.add(id)
      for (;;) {
        const version = projectVersion
        await (sync ??= synchronize(version, async (specifier, importer) => {
          const resolved = await this.resolve(specifier, importer, { skipSelf: true })
          return resolved && !resolved.external ? parseVirtualId(resolved.id)?.sourcePath ?? resolved.id : null
        }).catch(error => { sync = undefined; throw error }))
        if (version === projectVersion) break
        sync = undefined
      }
      for (const module of sources.keys()) this.addWatchFile(fileFor(module))
      const entry = idFor(virtual.sourcePath)
      const result = compiler.compile(entry, false, virtual.kind === 'bundle')
      const artifacts = resultFor(result, error => this.error(error))
      for (const diagnostic of result.diagnostics) if (diagnostic.severity === 'warning') this.warn({ message: diagnostic.message, id: fileFor(diagnostic.moduleId) })
      const artifact = artifacts.get(entry)
      if (!artifact) this.error('XCSS did not produce the requested module')
      if (virtual.kind !== 'module') {
        const map = JSON.parse(artifact.sourceMap)
        // Engine maps use project-relative identities, while Vite resolves map
        // sources relative to the virtual stylesheet's directory.
        map.sources = map.sources.map((source: string) => fileFor(source))
        map.sourceRoot = ''
        return { code: artifact.css, map, moduleSideEffects: true }
      }
      if (virtual.mode === 'url' && !build) {
        const direct = createCssId(virtual.sourcePath, true, 'direct')
        const path = virtual.sourcePath.startsWith(normalizePath(root) + '/')
          ? direct.slice(normalizePath(root).length + 1)
          : '@fs/' + direct
        return { code: 'export default ' + JSON.stringify(base + path) + ';', map: null }
      }
      if (virtual.mode !== 'module') return { code: 'export { default } from ' + JSON.stringify(createCssId(virtual.sourcePath, true, virtual.mode)) + ';', map: null }
      const imports: string[] = []
      const seen = new Set<string>()
      const visit = (moduleId: string) => {
        if (seen.has(moduleId)) return
        seen.add(moduleId)
        const module = artifacts.get(moduleId)
        if (!module) this.error('Missing composition artifact: ' + moduleId)
        for (const dependency of module.stylesheetDependencies) visit(dependency)
        imports.push('import ' + JSON.stringify(createCssId(fileFor(moduleId))) + ';')
      }
      visit(entry)
      return { code: imports.join('\n') + '\nexport default JSON.parse(' + JSON.stringify(JSON.stringify(artifact.classes)) + ');', map: null, moduleSideEffects: true }
    },
    handleHotUpdate(context) {
      if (!context.file.toLowerCase().endsWith('.xcss')) return
      invalidate(context.server)
      const affected = new Set<ModuleNode>(context.modules)
      for (const id of activeIds) { const module = context.server.moduleGraph.getModuleById(id); if (module) affected.add(module) }
      return [...affected]
    },
  }
}
async function findXcssFiles(root: string): Promise<string[]> {
  const result: string[] = []
  async function visit(directory: string): Promise<void> {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      if (entry.isSymbolicLink()) continue
      if (entry.isDirectory() && ['node_modules', '.git', 'dist', 'target', 'coverage', '.xcss'].includes(entry.name)) continue
      const path = join(directory, entry.name)
      if (entry.isDirectory()) await visit(path)
      else if (entry.isFile() && path.toLowerCase().endsWith('.xcss')) result.push(normalizePath(path))
    }
  }
  await visit(root)
  return result.sort()
}
export { xcss }
