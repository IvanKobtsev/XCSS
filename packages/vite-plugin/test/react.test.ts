import { fileURLToPath } from 'node:url'
import { build, createServer, type InlineConfig } from 'vite'
import { expect, it } from 'vitest'
import xcss from '../src/index.js'
import { ms, percentage, px, vh } from '../src/units.js'

const root = fileURLToPath(new URL('./fixtures/react/', import.meta.url))
interface Fixture {
  cssVars: Readonly<Record<string, string>>
  emptyVars: Readonly<Record<string, string>>
  html: string
  ref: unknown
  element: { key: string | null; props: { ref: unknown } }
  empty: { props: { style: Record<string, string> } }
  unchanged: { props: { style: Record<string, string> } }
}

function checkFixture(app: Fixture) {
  const width = app.cssVars.providedWidth
  expect(width).toMatch(/^--provided-width-xcss-/)
  expect(Object.keys(app.cssVars)).toEqual(['angle', 'providedWidth'])
  expect(app.emptyVars).toEqual({})
  expect(app.html).toContain(`height:200px;${width}:24px`)
  expect(app.html).toContain(`style="${width}:24px"`)
  expect(app.html).toContain('<b>A</b><i>B</i>')
  expect(app.html).toContain('<span>24px</span>')
  expect(app.html).not.toContain('xVars')
  expect(app.element.key).toBe('box')
  expect(app.element.props.ref).toBe(app.ref)
  expect(app.empty.props.style).toBe(app.unchanged.props.style)
  expect(app.unchanged.props.style[width]).toBe('1px')
}

function config(): InlineConfig {
  return { root, configFile: false, plugins: [xcss()], logLevel: 'silent' }
}

it('merges xVars in the Vite development runtime and SSR', async () => {
  const server = await createServer({ ...config(), server: { middlewareMode: true }, appType: 'custom' })
  try {
    const app = await server.ssrLoadModule('/app.tsx') as Fixture
    checkFixture(app)
  } finally {
    await server.close()
  }
})

it('merges xVars in a production bundle', async () => {
  const result = await build({
    ...config(),
    build: {
      write: false,
      lib: { entry: 'app.tsx', formats: ['es'], fileName: 'react-fixture' },
    },
  })
  const outputs = Array.isArray(result) ? result : [result]
  const files = outputs.flatMap(output => 'output' in output ? output.output : [])
  const entry = files.find(file => file.type === 'chunk' && file.isEntry)
  const css = files.flatMap(file => file.type === 'asset' && file.fileName.endsWith('.css') ? [file.source] : []).join('\n')
  if (entry?.type !== 'chunk') throw new Error('Missing Vite entry')
  const app = await import(/* @vite-ignore */ `data:text/javascript;base64,${Buffer.from(entry.code).toString('base64')}`) as Fixture
  checkFixture(app)
  expect(css).toContain(`var(${app.cssVars.providedWidth})`)
})

it('formats finite CSS numbers with their unit', () => {
  expect(px(' -.5 ')).toBe('-.5px')
  expect(percentage(75)).toBe('75%')
  expect(vh(10)).toBe('10vh')
  expect(ms('1e+2')).toBe('1e+2ms')
  for (const value of ['', '1px', '0x10', '1.', Infinity, NaN]) {
    expect(() => px(value)).toThrow(TypeError)
  }
})
