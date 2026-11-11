import ts from 'typescript'
import { normalizePath } from 'vite'

export function hasIntrinsicXClass(file: string, text: string): boolean {
  const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest)
  function visit(node: ts.Node): boolean {
    if ((ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node)) &&
      ts.isIdentifier(node.tagName) && (/^[a-z]/.test(node.tagName.text) || node.tagName.text.includes('-')) &&
      node.attributes.properties.some(attribute => ts.isJsxAttribute(attribute) &&
        ts.isIdentifier(attribute.name) && attribute.name.text === 'xClass')) return true
    return ts.forEachChild(node, visit) ?? false
  }
  return visit(source)
}

export function classProject(config: string, idFor: (file: string) => string) {
  const sources = new Map<string, string>()
  const readFile = (file: string) => {
    const text = ts.sys.readFile(file)
    if (text !== undefined) sources.set(idFor(file), text)
    return text
  }
  const host: ts.ParseConfigFileHost = {
    ...ts.sys,
    readFile,
    onUnRecoverableConfigFileDiagnostic: diagnostic => {
      throw new Error(ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n'))
    },
  }
  const parsed = ts.getParsedCommandLineOfConfigFile(config, {}, host)
  if (!parsed) throw new Error('Cannot load XCSS TypeScript project ' + config)
  if (parsed.errors.length) throw new Error(parsed.errors.map(error =>
    ts.flattenDiagnosticMessageText(error.messageText, '\n')).join('\n'))
  const compilerHost = ts.createCompilerHost(parsed.options)
  compilerHost.readFile = readFile
  const program = ts.createProgram({
    rootNames: parsed.fileNames,
    options: parsed.options,
    host: compilerHost,
    projectReferences: parsed.projectReferences,
  })
  const root = new Set(parsed.fileNames.map(normalizePath))
  const hasJsx = program.getSourceFiles().some(file =>
    /\.[jt]sx$/i.test(file.fileName) && (root.has(normalizePath(file.fileName)) || !program.isSourceFileFromExternalLibrary(file)))
  return {
    hasJsx,
    sources: [...sources].map(([path, text]) => ({ path, text })),
  }
}
