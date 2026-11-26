import { fileURLToPath } from 'node:url'
import ts from 'typescript'
import { expect, it } from 'vitest'

function consumer(source: string, moduleResolution = ts.ModuleResolutionKind.NodeNext) {
  const file = fileURLToPath(new URL('./types-consumer.ts', import.meta.url))
  const options: ts.CompilerOptions = {
    noEmit: true,
    strict: true,
    target: ts.ScriptTarget.ESNext,
    module: moduleResolution === ts.ModuleResolutionKind.NodeNext ? ts.ModuleKind.NodeNext : ts.ModuleKind.ESNext,
    moduleResolution,
    types: [],
  }
  const host = ts.createCompilerHost(options)
  const same = (path: string) => path.replaceAll('\\', '/') === file.replaceAll('\\', '/')
  const exists = host.fileExists.bind(host)
  const read = host.readFile.bind(host)
  const getSourceFile = host.getSourceFile.bind(host)
  host.fileExists = path => same(path) || exists(path)
  host.readFile = path => same(path) ? source : read(path)
  host.getSourceFile = (path, languageVersion, onError, shouldCreateNewSourceFile) => same(path)
    ? ts.createSourceFile(path, source, languageVersion)
    : getSourceFile(path, languageVersion, onError, shouldCreateNewSourceFile)
  const client = fileURLToPath(new URL('../client.d.ts', import.meta.url))
  return ts.createProgram([client, file], options, host)
}

it.each([
  ['NodeNext', ts.ModuleResolutionKind.NodeNext],
  ['Bundler', ts.ModuleResolutionKind.Bundler],
] as const)('resolves shared class types without framework declarations under %s', (_, resolution) => {
  const program = consumer(`
    import type { ClassName, XClass } from '@cross-css/vite/types';
    import { styles } from './Button.xcss';
    const name: ClassName = styles.button;
    const classes: XClass = [name, false, null, undefined] as const;
    const text: string = name;
    // @ts-expect-error Ordinary strings are not branded stylesheet classes.
    const invalid: ClassName = 'button';
  `, resolution)
  expect(ts.getPreEmitDiagnostics(program).map(d => ts.flattenDiagnosticMessageText(d.messageText, '\n'))).toEqual([])
  expect(program.getSourceFiles().map(file => file.fileName.replaceAll('\\', '/'))
    .filter(path => /\/(?:@types\/react|react|solid-js|@solidjs)\//.test(path))).toEqual([])
})

it('requires shared class type imports to use the neutral entry point', () => {
  const program = consumer(`
    import type { ClassName as ReactClassName } from '@cross-css/vite/react';
    import type { ClassName as SolidClassName } from '@cross-css/vite/solidjs';
  `)
  const diagnostics = ts.getPreEmitDiagnostics(program)
  expect(diagnostics.map(d => d.code)).toEqual([2305, 2305])
  expect(diagnostics.every(d => ts.flattenDiagnosticMessageText(d.messageText, '\n').includes('ClassName'))).toBe(true)
})
