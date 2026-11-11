import { fileURLToPath } from 'node:url'
import ts from 'typescript'
import { expect, it } from 'vitest'

it('keeps ordinary TypeScript permissive for XCSS classes and variant values', () => {
  const file = fileURLToPath(new URL('./client-consumer.ts', import.meta.url))
  const client = fileURLToPath(new URL('../client.d.ts', import.meta.url))
  const options: ts.CompilerOptions = { noEmit: true, strict: true, target: ts.ScriptTarget.ESNext, types: [], skipLibCheck: false }
  const host = ts.createCompilerHost(options)
  const read = host.readFile.bind(host)
  const exists = host.fileExists.bind(host)
  host.fileExists = path => path === file || exists(path)
  const consumer = `
    import { styles, type Variants } from './Button.xcss';
    const value: Variants<'UnknownGroup'> = 'anything';
    const name: string = styles.nonExisting;
    export { value, name };
  `
  const same = (path: string) => path.replaceAll('\\', '/') === file.replaceAll('\\', '/')
  host.readFile = path => same(path) ? consumer : read(path)
  const getSourceFile = host.getSourceFile.bind(host)
  host.getSourceFile = (path, languageVersion, onError, shouldCreateNewSourceFile) => same(path)
    ? ts.createSourceFile(path, consumer, languageVersion)
    : getSourceFile(path, languageVersion, onError, shouldCreateNewSourceFile)
  const program = ts.createProgram([client, file], options, host)
  expect(ts.getPreEmitDiagnostics(program).map(d => ts.flattenDiagnosticMessageText(d.messageText, '\n'))).toEqual([])
})
