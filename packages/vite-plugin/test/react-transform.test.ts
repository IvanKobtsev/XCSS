import ts from 'typescript'
import * as React from 'react'
import * as runtime from 'react/jsx-runtime'
import { expect, it, vi } from 'vitest'
import { transformReact } from '../src/react-transform.js'
import * as xcss from '../src/react/index.js'

function run(source: string, transform = true) {
  const code = transform ? transformReact(source, 'test.tsx')?.code ?? source : source
  const js = ts.transpileModule(code, { compilerOptions: {
    target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX,
  } }).outputText
  const exports: Record<string, any> = {}
  new Function('require', 'exports', js)((id: string) => {
    if (id === '@cross-css/vite/react') return xcss
    if (id === 'react/jsx-runtime') return runtime
    if (id === 'react') return React
    throw new Error(id)
  }, exports)
  return exports
}

it('preserves spread reads, evaluation order, duplicate props, keys, refs and children', () => {
  const source = `
    export const log = [];
    const value = (name, result) => (log.push(name), result);
    export const ref = { current: null };
    const spread = { get title() { log.push('getter'); return 'spread'; }, key: 'spread-key', ref };
    export const element = <div title={value('first', 'first')} key={value('key', 'key')}
      {...value('spread', spread)} title={value('last', 'last')} {...null} {...undefined}
      xVars={value('vars', { '--width': '2px' })} style={value('style', { height: 3 })}>
      {value('child', <b />)}{value('second-child', 'text')}
    </div>;
    export const afterKey = <div {...spread} key={value('after-key', 'last-key')} />;
  `
  const actual = run(source)
  const warning = vi.spyOn(console, 'error').mockImplementation(() => {})
  let baseline: ReturnType<typeof run>
  try { baseline = run(source, false) } finally { warning.mockRestore() }
  expect(actual.log).toEqual(baseline.log)
  expect(actual.log).toEqual(['first', 'spread', 'getter', 'last', 'vars', 'style', 'child', 'second-child', 'key', 'getter', 'after-key'])
  expect(actual.element.key).toBe(baseline.element.key)
  expect(actual.afterKey.key).toBe(baseline.afterKey.key)
  expect(actual.element.props.ref).toBe(actual.ref)
  expect(actual.element.props.title).toBe('last')
  expect(actual.element.props.style).toEqual({ height: 3, '--width': '2px' })
  expect(actual.element.props.children).toEqual(baseline.element.props.children)
  expect(actual.element.props).not.toHaveProperty('xVars')
})

it('uses the final xVars and style values, copies spreads immediately, and preserves source objects', () => {
  const app = run(`
    export const vars = { '--width': '1px' };
    export const style = { color: 'red' };
    const props = { title: 'before', xVars: vars, style };
    export const element = <div {...props} data-change={(props.title = 'after')}
      xVars={{ '--width': '2px' }} style={{ color: 'blue' }} />;
    export const cleared = <div {...props} xVars={undefined} />;
  `)
  expect(app.element.props.title).toBe('before')
  expect(app.element.props.style).toEqual({ color: 'blue', '--width': '2px' })
  expect(app.vars).toEqual({ '--width': '1px' })
  expect(app.style).toEqual({ color: 'red' })
  expect(app.cleared.props.style).toBe(app.style)
  expect(app.cleared.props).not.toHaveProperty('xVars')
})

it('retains JSX text, entities, nested elements, custom components and helper name hygiene', () => {
  const source = `
    const __xcssReactElement = 'original';
    const __xcssExpression = 'original';
    function Component(props) { return null; }
    export const element = <section xVars={{}} title="A &amp; B" hidden data-name={__xcssReactElement}
      data-factory="_jsx" data-marker="__xcssExp&#114;ession_0">
      Hello &lt;world&gt;
      <Component xVars={{ '--x': 1 }}><i {...{ xVars: { '--x': 2 } }} /></Component>
      <svg {...{}}><path d="M0 0" /></svg>
    </section>;
  `
  const actual = run(source).element
  const baseline = run(source, false).element
  expect(actual.props.title).toBe('A & B')
  expect(actual.props.hidden).toBe(true)
  expect(actual.props['data-factory']).toBe('_jsx')
  expect(actual.props['data-marker']).toBe('__xcssExpression_0')
  expect(actual.props['data-name']).toBe('original')
  expect(actual.props.children[0]).toBe(baseline.props.children[0])
  expect(actual.props.children[1].props.xVars).toEqual({ '--x': 1 })
  expect(actual.props.children[1].props.children.props.style).toEqual({ '--x': 2 })
  expect(actual.props.children[2]).toEqual(baseline.props.children[2])
})

it('keeps await and yield in their original scopes', async () => {
  const app = run(`
    export async function load() { return <div xVars={await Promise.resolve({ '--x': 1 })}>{await Promise.resolve('child')}</div>; }
    export function* generate() { return <div xVars={yield 'vars'}>{yield 'child'}</div>; }
  `)
  expect((await app.load()).props).toEqual({ style: { '--x': 1 }, children: 'child' })
  const iterator = app.generate()
  expect(iterator.next().value).toBe('vars')
  expect(iterator.next({ '--x': 2 }).value).toBe('child')
  expect(iterator.next('text').value.props).toEqual({ style: { '--x': 2 }, children: 'text' })
})

it('preserves comments, directives, and files with no affected elements', () => {
  const code = '#!/usr/bin/env node\n/** @jsxImportSource react */\n"use client";\nexport const node = <div xVars={{}} />'
  const output = transformReact(code, 'test.tsx')!.code
  expect(output).toContain('/** @jsxImportSource react */')
  expect(output).toContain('"use client";')
  expect(output.indexOf('"use client";')).toBeLessThan(output.indexOf('import {'))
  expect(output.startsWith('#!/usr/bin/env node\n')).toBe(true)
  expect(transformReact('export const node = <Component {...props} />', 'test.tsx')).toBeUndefined()
})

it('does not defer spread getters and stops evaluation at a thrown getter', () => {
  const app = run(`
    export const log = [];
    try {
      const element = <div {...{ get title() { log.push('get'); throw new Error('stop'); } }}
        xVars={(log.push('vars'), {})}>{log.push('child')}</div>;
    } catch (error) { log.push(error.message); }
  `)
  expect(app.log).toEqual(['get', 'stop'])
})

it('normalizes only after child expressions have evaluated', () => {
  const app = run(`
    const vars = { '--x': 'before' };
    export const element = <div xVars={vars}>{(vars['--x'] = 'after', 'text')}</div>;
  `)
  expect(app.element.props).toEqual({ children: 'text', style: { '--x': 'after' } })
})
