import { fileURLToPath } from 'node:url'
import { cp, mkdtemp, readFile, rename, rm, writeFile } from 'node:fs/promises'
import { dirname, join } from 'node:path'
import { build, createServer, type InlineConfig } from 'vite'
import { expect, it } from 'vitest'
import xcss from '../src/index.js'
import { ms, percentage, px, vh } from '../src/units.js'
import { normalizeProps } from '../src/react/props.js'
import { createCssId } from '../src/internal.js'
import { compilerTimeout, restartTimeout } from './integration-timeouts.js'

const root = fileURLToPath(new URL('./fixtures/react/', import.meta.url))
interface Fixture {
  cssVars: Readonly<Record<string, string>>
  emptyVars: Readonly<Record<string, string>>
  styles: Record<string, string>
  html: string
  boosted: string
  overriddenHtml: string
  variantHtml: string
  originalVariant: string
  ref: unknown
  element: { key: string | null; props: { ref: unknown } }
  empty: { props: { style: Record<string, string>; className?: string } }
  unchanged: { props: { style: Record<string, string>; className?: string } }
}

function checkFixture(app: Fixture) {
  const width = app.cssVars.providedWidth
  expect(width).toMatch(/^--provided-width_[0-9a-z]{3}$/)
  expect(Object.keys(app.cssVars)).toEqual(['angle', 'providedWidth'])
  expect(app.emptyVars).toEqual({})
  expect(app.html).toContain(`height:200px;${width}:24px`)
  expect(app.html).toContain(`style="${width}:24px" class="${app.styles.box}"`)
  expect(app.html).toContain('<b>A</b><i>B</i>')
  expect(app.variantHtml).toContain('class="secondary_000"')
  expect(app.originalVariant).toBe('secondary')
  expect(app.boosted).toBe('override_000 override_000_override_2')
  expect(app.overriddenHtml).toContain('box_000 override_000 override_000_override_1')
  expect(app.html).toContain('<span>24px</span>')
  expect(app.html).not.toContain('xVars')
  expect(app.html.toLowerCase()).not.toContain('xclass')
  expect(app.html).toContain('class="global ' + app.styles.box + ' ' + app.styles.box + '"')
  expect(app.html).toMatch(/id="first" class="firstButton_000 secondary_000 override_000"/)
  expect(app.html).toMatch(/id="second" class="secondButton_000 override_000 secondary_000"/)
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
}, compilerTimeout)

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
  expect(css).toContain('primary_000')
  expect(css).toContain('override_000_override_2')
  expect(css).toContain('#_#_')
}, compilerTimeout)

it('refreshes composition and override levels without renumbering base classes', async () => {
  const temporary = await mkdtemp(join(dirname(root.replace(/[\\/]$/, '')), 'react-update-'))
  await cp(root, temporary, { recursive: true })
  await rename(join(temporary, 'tsconfig.json'), join(temporary, 'tsconfig.app.json'))
  await writeFile(join(temporary, 'xcss.config.json'), JSON.stringify({ tsconfig: './tsconfig.app.json' }))
  const server = await createServer({
    ...config(), root: temporary, server: { middlewareMode: true }, appType: 'custom',
    plugins: [xcss()],
  })
  try {
    const appPath = join(temporary, 'app.tsx')
    const cssId = createCssId(join(temporary, 'box.xcss').replaceAll('\\', '/'))
    const first = await server.ssrLoadModule('/app.tsx') as Fixture
    expect(first.html).toContain('global box_000 box_000')
    const css = await server.transformRequest(cssId)
    expect(css?.code).toContain('box_000')
    const source = await readFile(appPath, 'utf8')
    await writeFile(appPath, source.replace('[styles.box, false, styles.box]', '[styles.box, false, false, styles.box]').replace('<b>A</b>', '<b>Updated</b>'))
    await expect.poll(async () => {
      const app = await server.ssrLoadModule('/app.tsx') as Fixture
      expect(app.html).toContain('global box_000 box_000')
      expect(app.html).toContain('<b>Updated</b>')
      return (await server.transformRequest(cssId))?.code
    }).toBe(css?.code)
    const refreshed = await server.transformRequest(cssId)
    expect(refreshed?.code).toBe(css?.code)
    expect(refreshed?.code).not.toContain('#__')
    const helperPath = join(temporary, 'overrides.ts')
    const helper = await readFile(helperPath, 'utf8')
    await writeFile(helperPath, helper.replace('boost(styles.override, 2)', 'boost(styles.override, 5)'))
    await expect.poll(async () => {
      const app = await server.ssrLoadModule('/app.tsx') as Fixture
      expect(app.boosted).toBe('override_000 override_000_override_5')
      const updated = await server.transformRequest(cssId)
      expect(updated?.code).not.toContain('override_000_override_2')
      return updated?.code
    }).toContain('override_000_override_5')
    await writeFile(helperPath, helper.replace('boost(styles.override, 2)', 'styles.override'))
    await expect.poll(async () => {
      const app = await server.ssrLoadModule('/app.tsx') as Fixture
      expect(app.boosted).toBe('override_000')
      return (await server.transformRequest(cssId))?.code
    }).not.toContain('override_000_override_5')
  } finally {
    await server.close()
    await rm(temporary, { recursive: true, force: true })
  }
}, compilerTimeout)

it('normalizes clsx values and preserves custom component props', () => {
  const props = { className: 'global', xClass: ['local', false, 'local'] }
  expect(normalizeProps('div', props)).toEqual({ className: 'global local local' })
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


it('restarts and reloads scoped names when shared configuration changes', async () => {
  const temporary = await mkdtemp(join(dirname(root), 'react-config-'))
  await cp(root, temporary, { recursive: true })
  const configPath = join(temporary, 'xcss.config.json')
  await writeFile(configPath, JSON.stringify({ suffixLength: 4 }))
  const server = await createServer({ ...config(), root: temporary, server: { middlewareMode: true }, appType: 'custom' })
  try {
    const first = await server.ssrLoadModule('/app.tsx') as Fixture
    expect(first.styles.box).toBe('box_0000')
    await writeFile(configPath, JSON.stringify({ suffixLength: 5 }))
    await expect.poll(async () => {
      try { return (await server.ssrLoadModule('/app.tsx') as Fixture).styles.box }
      catch { return undefined }
    }, { timeout: 10000 }).toBe('box_00000')
    await rm(configPath)
    await expect.poll(async () => {
      try { return (await server.ssrLoadModule('/app.tsx') as Fixture).styles.box }
      catch { return undefined }
    }, { timeout: 10000 }).toBe('box_000')
  } finally {
    await server.close()
    await rm(temporary, { recursive: true, force: true })
  }
}, restartTimeout)
