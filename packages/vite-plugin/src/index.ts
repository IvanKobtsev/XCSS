import { readFile, readdir, stat } from 'node:fs/promises'
import { basename, dirname, join, posix, relative, resolve as resolvePath } from 'node:path'
import { Compiler, inspect } from '@cross-css/native'
import type { NativeArtifact, NativeClassPlan, NativeCompilation } from '@cross-css/native'
import { normalizePath, type Plugin, type ModuleNode, type ViteDevServer } from 'vite'
import { createCssId, createVirtualId, cssAssetUrls, isCrossCssId, parseVirtualId } from './internal.js'
import { classProject, hasIntrinsicXClass } from './class-project.js'

const projectSource = /\.(?:xcss|[jt]sx?|[cm][jt]s|json)$/i

export default function xcss(options: { tsconfig?: string } = {}): Plugin {
  const compiler = new Compiler()
  const sources = new Map<string, { source: string; revision: number }>()
  const activeIds = new Set<string>()
  const utilityEntries = new Set<string>()
  const utilityModuleId = '\0xcss-shared-utilities'
  let utilityAsset: string | undefined
  let utilityFile: string | undefined
  let devServer: ViteDevServer | undefined
  let root = process.cwd()
  let build = false
  let base = '/'
  let assetsDir = 'assets'
  let outDir = 'dist'
  let configFiles = new Set<string>()
  let revision = 0
  let projectVersion = 0
  let sync: Promise<void> | undefined
  let classPlan: NativeClassPlan | undefined
  let classError: Error | undefined
  const jsxModules = new Set<string>()
  const idFor = (path: string) => normalizePath(relative(root, path))
  const fileFor = (id: string) => normalizePath(resolvePath(root, id))
  const utilityCssId = () => normalizePath(join(root, '__xcss_utils.css'))
  const invalidate = (server: ViteDevServer) => {
    projectVersion++
    sync = undefined
    for (const id of [...activeIds, ...jsxModules]) { const module = server.moduleGraph.getModuleById(id); if (module) server.moduleGraph.invalidateModule(module) }
    const shared = server.moduleGraph.getModuleById(utilityCssId())
    if (shared) server.moduleGraph.invalidateModule(shared)
  }
  async function synchronize(version: number) {
    const config = options.tsconfig ? resolvePath(root, options.tsconfig) : undefined
    configFiles = new Set(compiler.configureProject(root, config))
    if (devServer) devServer.watcher.add([...configFiles])
    const filenames = await findXcssFiles(root)
    const loaded = await Promise.all(filenames.map(async filename => ({ filename, source: await readFile(filename, 'utf8') })))
    const resolutions: Array<{ importer: string; specifier: string; target: string | null }> = []
    for (const file of loaded) for (const specifier of inspect(file.source).imports) {
      let resolved: string | null
      try { resolved = compiler.resolveImport(file.filename, specifier) }
      catch (error) {
        const line = file.source.slice(0, file.source.indexOf(specifier)).split('\n').length
        throw new Error(file.filename + ':' + line + ': ' + (error instanceof Error ? error.message : String(error)))
      }
      resolutions.push({ importer: idFor(file.filename), specifier, target: resolved ? idFor(resolved.split('?')[0]) : null })
    }
    if (version !== projectVersion) return
    const current = new Set(loaded.map(file => idFor(file.filename)))
    for (const id of sources.keys()) if (!current.has(id)) { compiler.removeFile(id, ++revision); sources.delete(id) }
    for (const id of utilityEntries) if (!current.has(id)) utilityEntries.delete(id)
    for (const file of loaded) {
      const id = idFor(file.filename)
      compiler.setFile(id, ++revision, file.source); sources.set(id, { source: file.source, revision })
    }
    for (const resolution of resolutions) compiler.setResolution(resolution.importer, resolution.specifier, resolution.target)
    classPlan = undefined
    classError = undefined
    compiler.clearClassSlots()
    const classConfig = resolvePath(root, options.tsconfig ?? 'tsconfig.json')
    try {
      await stat(classConfig)
      if (version !== projectVersion) return
      const prefix = '_/'.repeat(normalizePath(root).split('/').length) + '__xcss_project_root__'
      const projectId = (file: string) => posix.join(prefix, idFor(file))
      const project = classProject(classConfig, projectId)
      if (project.hasJsx) classPlan = compiler.planClassSlots(projectId(classConfig), project.sources)
    } catch (error) {
      if (version === projectVersion) classError = error instanceof Error ? error : new Error(String(error))
    }
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
    buildStart() { sync = undefined; projectVersion++; utilityEntries.clear(); utilityAsset = undefined; utilityFile = undefined; this.addWatchFile(root) },
    configResolved(config) { root = config.root; build = config.command === 'build'; base = config.base; assetsDir = config.build.assetsDir; outDir = config.build.outDir },
    configureServer(server) {
      devServer = server
      const changed = (file: string) => {
        if (projectSource.test(file) || configFiles.has(normalizePath(file))) {
          invalidate(server)
          server.ws.send({ type: 'full-reload' })
        }
      }
      const edited = (file: string) => { if (projectSource.test(file) || configFiles.has(normalizePath(file))) invalidate(server) }
      server.watcher.on('add', changed)
      server.watcher.on('unlink', changed)
      server.watcher.on('change', edited)
      server.httpServer?.once('close', () => { server.watcher.off('add', changed); server.watcher.off('unlink', changed); server.watcher.off('change', edited) })
    },
    async transform(code, id) {
      if (!/\.[jt]sx$/i.test(id)) return
      jsxModules.add(id)
      for (;;) {
        const version = projectVersion
        await (sync ??= synchronize(version).catch(error => { sync = undefined; throw error }))
        if (version === projectVersion) break
        sync = undefined
      }
      for (const config of configFiles) this.addWatchFile(config)
      if (classError && (options.tsconfig || hasIntrinsicXClass(id, code))) this.error(classError.message)
      const file = idFor(id)
      const diagnostic = classPlan?.diagnostics.find(item => ['XC-001', 'XC-002', 'XC-003', 'XC-009'].includes(item.code) && item.moduleId === file)
      if (diagnostic) this.error({ message: diagnostic.message, id, loc: { file: id, line: diagnostic.line, column: diagnostic.column - 1 } })
      const sites = classPlan?.sites.filter(site => site.moduleId === file)
      if (!sites?.length) {
        if (hasIntrinsicXClass(id, code)) this.error('The XCSS TypeScript project must include this JSX file')
        return
      }
      const jsxSource = /@jsxImportSource\s+([^\s*]+)/.exec(code)?.[1]
      if (jsxSource && jsxSource !== '@cross-css/vite/react') {
        this.error('xClass requires the @cross-css/vite/react JSX runtime')
      }
      const imports = new Set<string>()
      const modules = new Map<string, NativeArtifact>()
      let output = code
      for (const site of [...sites].sort((a, b) => b.offset - a.offset)) {
        if (code[site.offset] !== '>' && code.slice(site.offset, site.offset + 2) !== '/>') {
          this.error('XCSS could not locate the analyzed JSX element. Run XCSS before plugins that rewrite JSX.')
        }
        const slots = site.slots.map(slot => slot.map(origin => {
          let module = modules.get(origin.moduleId)
          if (!module) {
            module = resultFor(compiler.compile(origin.moduleId), error => this.error(error)).get(origin.moduleId)
            if (!module) this.error('Missing XCSS class module ' + origin.moduleId)
            modules.set(origin.moduleId, module)
          }
          let specifier = normalizePath(relative(dirname(id), fileFor(origin.moduleId)))
          if (!specifier.startsWith('.')) specifier = './' + specifier
          imports.add(specifier)
          const baseClass = module.classes[origin.className]
          if (baseClass === undefined) this.error('Missing XCSS class ' + origin.className)
          return [origin.variantLabel ?? baseClass, baseClass + ' ' + origin.alias]
        }))
        output = output.slice(0, site.offset) + ' __xcssClassSlots={' + JSON.stringify(slots) + '} ' + output.slice(site.offset)
      }
      const prefix = jsxSource ? '' : '/** @jsxImportSource @cross-css/vite/react */\n'
      const firstLine = output.startsWith('#!') ? output.indexOf('\n') + 1 : 0
      const importStatements = [...imports].map(path => 'import ' + JSON.stringify(path) + ';')
      return {
        code: output.slice(0, firstLine) + prefix + output.slice(firstLine) + '\n' + importStatements.join('\n'),
        map: null,
      }
    },
    async resolveId(source, importer, options) {
      if (source === utilityModuleId || source === utilityCssId()) return source
      if (parseVirtualId(source)) return source
      if (!isCrossCssId(source)) return null
      const resolved = await this.resolve(source, importer, { ...options, skipSelf: true })
      if (!resolved || resolved.external) return null
      const virtual = parseVirtualId(resolved.id) ?? createVirtualId(resolved.id)
      if (virtual) activeIds.add(virtual.id)
      return virtual?.id ?? null
    },
    async load(id) {
      if (id === utilityModuleId) {
        utilityAsset = 'pending'
        return { code: `if (typeof document !== 'undefined') { const href = new URL("__XCSS_SHARED_UTILITIES_URL__", document.baseURI).href; if (![...document.querySelectorAll('link[rel="stylesheet"]')].some(link => link.href === href)) { const link = document.createElement('link'); link.rel = 'stylesheet'; link.href = href; document.head.append(link); } }`, moduleSideEffects: true }
      }
      if (id === utilityCssId()) {
        // Refresh from source on every shared-sheet request, including HMR removals.
        await (sync ??= synchronize(projectVersion).catch(error => { sync = undefined; throw error }))
        const result = compiler.compileUtilities([...utilityEntries].sort())
        resultFor(result, error => this.error(error))
        const shared = result.utilities
        if (!shared) return { code: '', map: null, moduleSideEffects: true }
        const map = JSON.parse(shared.sourceMap)
        map.sources = map.sources.map((source: string) => fileFor(source)); map.sourceRoot = ''
        return { code: shared.css, map, moduleSideEffects: true }
      }
      const virtual = parseVirtualId(id)
      if (!virtual) return null
      // Vite's native CSS ?url loader emits and rewrites the stylesheet asset.
      if (virtual.kind !== 'module' && virtual.mode === 'url') return null
      activeIds.add(id)
      for (;;) {
        const version = projectVersion
        await (sync ??= synchronize(version).catch(error => { sync = undefined; throw error }))
        if (version === projectVersion) break
        sync = undefined
      }
      for (const config of configFiles) this.addWatchFile(config)
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
      if (virtual.kind === 'module') {
        const added = !utilityEntries.has(entry)
        utilityEntries.add(entry)
        // Keep the shared sheet imported even when empty, so adding the first util works through HMR.
        if (!build || result.utilities) imports.push('import ' + JSON.stringify(build ? utilityModuleId : utilityCssId()) + ';')
        if (added && devServer) {
          const shared = devServer.moduleGraph.getModuleById(utilityCssId())
          if (shared) {
            devServer.moduleGraph.invalidateModule(shared)
            devServer.ws.send({ type: 'update', updates: [{ type: 'js-update', path: shared.url, acceptedPath: shared.url, timestamp: Date.now() }] })
          }
        }
      }
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
      const dictionary = (values: Record<string, string>) => 'JSON.parse(' + JSON.stringify(JSON.stringify(values)) + ')'
      return {
        code: [...imports, 'export const styles = ' + dictionary(artifact.classes) + ';', 'export const cssVars = ' + dictionary(artifact.cssVars) + ';'].join('\n'),
        map: null,
        moduleSideEffects: true,
      }
    },
    async renderStart() {
      if (!utilityAsset) return
      const result = compiler.compileUtilities([...utilityEntries].sort())
      resultFor(result, error => this.error(error))
      let css = result.utilities?.css ?? ''
      // The shared artifact is emitted after graph discovery, so lazy modules participate too.
      const urls = cssAssetUrls(css)
      for (const match of urls.reverse()) {
        const url = match.url
        if (/^(?:[a-z][a-z\d+.-]*:|\/|#)/i.test(url)) continue
        const suffixStart = url.search(/[?#]/)
        const path = suffixStart === -1 ? url : url.slice(0, suffixStart)
        const suffix = suffixStart === -1 ? '' : url.slice(suffixStart)
        let decoded = path
        try { decoded = decodeURIComponent(path) } catch { /* Retain literal percent signs. */ }
        const filename = fileFor(decoded)
        const reference = this.emitFile({ type: 'asset', name: basename(filename), source: await readFile(filename) })
        const assetPath = this.getFileName(reference)
        const target = base === './' || base === '' ? relative(assetsDir, assetPath).replaceAll('\\', '/') : base + assetPath
        css = css.slice(0, match.start) + 'url(' + JSON.stringify(target + suffix) + ')' + css.slice(match.end)
      }
      if (result.utilities) {
        const map = JSON.parse(result.utilities.sourceMap)
        map.sources = map.sources.map((source: string) => normalizePath(relative(resolvePath(root, outDir, assetsDir), fileFor(source))))
        map.sourceRoot = ''
        const reference = this.emitFile({ type: 'asset', name: 'xcss-utils.css.map', source: JSON.stringify(map) })
        css += '\n/*# sourceMappingURL=' + normalizePath(relative(assetsDir, this.getFileName(reference))) + ' */\n'
      }
      utilityAsset = this.emitFile({ type: 'asset', name: 'xcss-utils.css', source: css })
      utilityFile = this.getFileName(utilityAsset)
    },
    renderChunk(code, chunk) {
      if (!utilityFile || !code.includes('__XCSS_SHARED_UTILITIES_URL__')) return null
      const href = base === './' || base === ''
        ? `new URL(${JSON.stringify(relative(dirname(chunk.fileName), utilityFile).replaceAll('\\', '/'))}, import.meta.url).href`
        : JSON.stringify(base + utilityFile)
      return { code: code.replace(/(["'])__XCSS_SHARED_UTILITIES_URL__\1/g, href), map: null }
    },
    transformIndexHtml: { order: 'post', handler() {
      if (!utilityFile) return
      return [{ tag: 'link', attrs: { rel: 'stylesheet', href: base + utilityFile }, injectTo: 'head' }]
    } },
    handleHotUpdate(context) {
      if (!projectSource.test(context.file) && !configFiles.has(normalizePath(context.file))) return
      invalidate(context.server)
      const affected = new Set<ModuleNode>(context.modules)
      for (const id of activeIds) { const module = context.server.moduleGraph.getModuleById(id); if (module) affected.add(module) }
      for (const id of jsxModules) { const module = context.server.moduleGraph.getModuleById(id); if (module) affected.add(module) }
      const shared = context.server.moduleGraph.getModuleById(utilityCssId())
      if (shared) affected.add(shared)
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
