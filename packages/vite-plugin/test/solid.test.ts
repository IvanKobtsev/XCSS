import { expect, test } from 'vitest'
import { solidProps } from '../src/solidjs.js'
import { transformSolid } from '../src/solid-transform.js'
import { build, createServer } from 'vite'
import solid from '@solidjs/vite-plugin'
import xcss from '../src/solidjs-plugin.js'
import { fileURLToPath } from 'node:url'
import { compilerTimeout } from './integration-timeouts.js'

const root = fileURLToPath(new URL('./fixtures/solid/', import.meta.url))
test('Solid compiler accepts XCSS in development and production', async () => {
  const config = { root, configFile: false as const, plugins: [xcss(), solid()], logLevel: 'silent' as const }
  const server = await createServer({ ...config, server: { middlewareMode: true } })
  try {
    const result = await server.transformRequest('/app.tsx')
    expect(result?.code).toContain('solidProps')
    expect(result?.code).toContain('_\u0024spread')
  } finally { await server.close() }
  const result = await build({ ...config, build: { write: false, lib: { entry: 'app.tsx', formats: ['es'] as ['es'] } } })
  const outputs = Array.isArray(result) ? result : [result]
  expect(outputs.some(output => 'output' in output && output.output.some(file => file.type === 'asset' && file.fileName.endsWith('.css')))).toBe(true)
  const css = outputs.flatMap(output => 'output' in output ? output.output.flatMap(file => file.type === 'asset' && file.fileName.endsWith('.css') ? [file.source] : []) : []).join('\n')
  expect(css).toContain('box_000')
  expect(css).not.toContain('box_001')
  expect(css).toContain('box_000_override_2')
  expect(css).toContain('#_#_')
}, compilerTimeout)

test('Solid bindings stay reactive and hide XCSS metadata', () => {
  let active = 'a'
  let width = '1px'
  const props = solidProps([{
    class: { existing: true },
    get xClass() { return active },
    get xVars() { return { '--width': width } },
  }])
  expect(props.class).toEqual([{ existing: true }, 'a'])
  active = 'b'
  width = '2px'
  expect(props.class).toEqual([{ existing: true }, 'b'])
  expect(props.style).toEqual({ '--width': '2px' })
  expect(Object.keys(props)).toEqual(['class', 'style'])
})

test('spread sources preserve ordering and dynamic replacement', () => {
  let spread = { xVars: { '--size': '1px' }, title: 'first' }
  const props = solidProps([() => spread, { title: 'last' }])
  spread = { xVars: { '--size': '2px' }, title: 'second' }
  expect(props.style).toEqual({ '--size': '2px' })
  expect(props.title).toBe('last')
})

test('transforms xVars-only elements and leaves components alone', () => {
  const result = transformSolid('<><div xVars={vars()} {...other} /><Button xClass={value} /></>', 'app.tsx')!
  expect(result.code).toContain('get "xVars"() { return (vars()); }')
  expect(result.code).toContain('() => (other)')
  expect(result.code).toContain('<Button xClass={value} />')
  expect(result.code).not.toContain('@jsxImportSource')
})

