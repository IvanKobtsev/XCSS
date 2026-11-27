import { test, expect } from '@playwright/test'
import { fileURLToPath } from 'node:url'
import { build } from 'vite'
import xcss from '@cross-css/vite/react-plugin'
import { Compiler } from '../../../packages/native/index.js'

test('flattened parent lists keep independent specificity and declaration order', async ({ page }) => {
  const compiler = new Compiler()
  compiler.setFile('nesting.xcss', 1, `
    .primary, .override:is(*, #boost) { &.secondary { color: red; } }
    .secondary.competitor { color: green; }
    .ordered { color: red; & { color: blue; } color: green; }
  `)
  const result = compiler.compile('nesting.xcss')
  expect(result.success).toBe(true)
  const { css, classes: c } = result.artifacts[0]
  expect(css).not.toContain('&')
  await page.setContent(`<style>${css}</style>
    <div id="normal" class="${c.primary} ${c.secondary} ${c.competitor}">Normal</div>
    <div id="override" class="${c.override} ${c.secondary} ${c.competitor}">Override</div>
    <div id="ordered" class="${c.ordered}">Ordered</div>`)
  await expect(page.locator('#normal')).toHaveCSS('color', 'rgb(0, 128, 0)')
  await expect(page.locator('#override')).toHaveCSS('color', 'rgb(255, 0, 0)')
  await expect(page.locator('#ordered')).toHaveCSS('color', 'rgb(0, 128, 0)')
})

test('xClass order does not change cross-module specificity', async ({ page }) => {
  const root = fileURLToPath(new URL('../../../packages/vite-plugin/test/fixtures/react/', import.meta.url))
  const result = await build({
    root,
    configFile: false,
    plugins: [xcss()],
    logLevel: 'silent',
    build: { write: false },
  })
  const outputs = (Array.isArray(result) ? result : [result]).flatMap(output => 'output' in output ? output.output : [])
  await page.route('http://xcss.test/**', async route => {
    const path = new URL(route.request().url()).pathname.slice(1) || 'index.html'
    const output = outputs.find(output => output.fileName === path)
    if (!output) return route.fulfill({ status: 404 })
    const contentType = path.endsWith('.html') ? 'text/html' : path.endsWith('.css') ? 'text/css' : 'text/javascript'
    await route.fulfill({ contentType, body: output.type === 'chunk' ? output.code : Buffer.from(output.source) })
  })
  await page.goto('http://xcss.test/')
  await expect(page.locator('#first')).toHaveCount(0)
  await page.locator('#load').click()
  await expect(page.locator('#first')).toHaveCSS('color', 'rgb(0, 0, 255)')
  await expect(page.locator('#second')).toHaveCSS('color', 'rgb(0, 0, 255)')
  await expect(page.locator('#second')).toHaveCSS('text-decoration-line', 'underline')
  await expect(page.locator('#second span')).toHaveCSS('font-weight', '700')
  await expect(page.locator('#base')).toHaveCSS('color', 'rgb(0, 128, 0)')
})
