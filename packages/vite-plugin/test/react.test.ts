import { fileURLToPath } from 'node:url'
import { cp, mkdtemp, readFile, rename, rm, writeFile } from 'node:fs/promises'
import { dirname, join } from 'node:path'
import { build, createServer, type InlineConfig } from 'vite'
import { expect, it } from 'vitest'
import xcss from '../src/index.js'
import { ms, percentage, px, vh } from '../src/units.js'
import { normalizeProps } from '../src/react/props.js'
import { createCssId } from '../src/internal.js'

const root = fileURLToPath(new URL('./fixtures/react/', import.meta.url))
interface Fixture {
  cssVars: Readonly<Record<string, string>>
  emptyVars: Readonly<Record<string, string>>
  styles: Record<string, string>
  html: string
  ref: unknown
  element: { key: string | null; props: { ref: unknown } }
  empty: { props: { style: Record<string, string>; className?: string } }
  unchanged: { props: { style: Record<string, string>; className?: string } }
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
  expect(app.html.toLowerCase()).not.toContain('xclass')
  expect(app.html).toContain('class="global ' + app.styles.box + ' _xcss_position_')
  expect(app.html).toContain('_626f78_2"')
  expect(app.html).toMatch(/id="first" class="[^"]*_7365636f6e64617279_1 [^"]*_6f76657272696465_2"/)
  expect(app.html).toMatch(/id="second" class="[^"]*_6f76657272696465_1 [^"]*_7365636f6e64617279_2"/)
  expect(app.empty.props).not.toHaveProperty('className')
  expect(app.unchanged.props.className).toBe(' kept ')
  expect(app.element.key).toBe('box')
  expect(app.element.props.ref).toBe(app.ref)
  expect(app.empty.props.style).toBe(app.unchanged.props.style)
  expect(app.unchanged.props.style[width]).toBe('1px')
}

function config(): InlineConfig {
  return { root, configFile: false, plugins: [xcss()], logLevel: 'silent' }
}

it('merges xVars and xClass in the Vite development runtime and SSR', async () => {
  const server = await createServer({ ...config(), server: { middlewareMode: true }, appType: 'custom' })
  try {
    const app = await server.ssrLoadModule('/app.tsx') as Fixture
    checkFixture(app)
  } finally {
    await server.close()
  }
})

it('merges xVars and xClass in a production bundle', async () => {
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
  expect(css).toContain('_7072696d617279_1')
  expect(css).toContain('_7072696d617279_2')
})

it('refreshes the slot plan and CSS when JSX changes in development', async () => {
  const temporary = await mkdtemp(join(dirname(root.replace(/[\\/]$/, '')), 'react-update-'))
  await cp(root, temporary, { recursive: true })
  await rename(join(temporary, 'tsconfig.json'), join(temporary, 'tsconfig.app.json'))
  const server = await createServer({
    ...config(), root: temporary, server: { middlewareMode: true }, appType: 'custom',
    plugins: [xcss({ tsconfig: 'tsconfig.app.json' })],
  })
  try {
    const appPath = join(temporary, 'app.tsx')
    const cssId = createCssId(join(temporary, 'box.xcss').replaceAll('\\', '/'))
    const first = await server.ssrLoadModule('/app.tsx') as Fixture
    expect(first.html).toContain('_626f78_2')
    const css = await server.transformRequest(cssId)
    expect(css?.code).toContain('_626f78_2')
    const source = await readFile(appPath, 'utf8')
    await writeFile(appPath, source.replace('[styles.box, false, styles.box]', '[styles.box, false, false, styles.box]'))
    await expect.poll(async () => {
      const app = await server.ssrLoadModule('/app.tsx') as Fixture
      return app.html
    }).toContain('_626f78_3')
    const refreshed = await server.transformRequest(cssId)
    expect(refreshed?.code).toContain('_626f78_3')
    expect(refreshed?.code).not.toContain('_626f78_2')
  } finally {
    await server.close()
    await rm(temporary, { recursive: true, force: true })
  }
})

it('preserves skipped slots and custom component props', () => {
  const props = { className: 'global', xClass: ['local', false, 'local'],
    __xcssClassSlots: [[['local', 'local local_0']], [], [['local', 'local local_2']]] }
  expect(normalizeProps('div', props)).toEqual({ className: 'global local local_0 local local_2' })
  expect(normalizeProps(() => null, props)).toBe(props)
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
