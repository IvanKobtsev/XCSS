import { test, expect } from '@playwright/test'
import { fileURLToPath } from 'node:url'
import { build } from 'vite'
import xcss from '@cross-css/vite'

test('xClass positions determine cross-module specificity', async ({ page }) => {
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
  await expect(page.locator('#first')).toHaveCSS('color', 'rgb(0, 128, 0)')
  await expect(page.locator('#second')).toHaveCSS('color', 'rgb(0, 0, 255)')
  await expect(page.locator('#second')).toHaveCSS('text-decoration-line', 'underline')
  await expect(page.locator('#second span')).toHaveCSS('font-weight', '700')
  await expect(page.locator('#base')).toHaveCSS('color', 'rgb(0, 128, 0)')
})
