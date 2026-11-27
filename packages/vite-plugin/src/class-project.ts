import ts from 'typescript'

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
  return {
    hasSources: program.getSourceFiles().some(file => !file.isDeclarationFile && /\.[cm]?[jt]sx?$/i.test(file.fileName)),
    sources: [...sources].map(([path, text]) => ({ path, text })),
  }
}
