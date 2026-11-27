import { test, expect } from '@playwright/test'
import { Compiler } from '../../../packages/native/index.js'

test('explicit overrides preserve function matching, negation, counting and layers', async ({ page }) => {
  const compiler = new Compiler()
  compiler.setFile('styles.xcss', 1, `
    @util padded { padding: 8px; }
    .a { @:padded; } .b {} .other {} .fallback {} .host {} .count {} .notTest {}
    .host:is(.a, .other) { color: red; }
    .host.other { color: green; }
    :where(.a) { background-color: red; }
    .other { background-color: green; }
    .withHas:has(> .a, > .other) { color: red; }
    .withHas.other { color: green; }
    .count:nth-child(2 of .a, .other) { color: red; }
    .count.other.fallback { color: green; }
    .notTest:not(.a) { display: none; }
    .a.b { border: 1px solid red; }
    :global(#combined).fallback.other.extra { border-color: green; }
    .a { outline-color: red; } .b { outline-color: blue; }
    .other { padding: 1px; }
  `)
  const plan = compiler.planClassUsage('tsconfig.json', [
    { path: 'tsconfig.json', text: '{"compilerOptions":{"module":"esnext","moduleResolution":"bundler"},"files":["app.ts"]}' },
    { path: 'app.ts', text: "import {override} from '@cross-css/vite'; import {styles} from './styles.xcss'; override(styles.a); override(styles.a, 2); override(styles.b);" },
  ])
  expect(plan.diagnostics).toEqual([])
  const result = compiler.compile('styles.xcss')
  expect(result.success).toBe(true)
  const { css, classes: c, overrides: o } = result.artifacts[0]
  const a = o.a['1'], a2 = o.a['2'], b = o.b['1']
  const markerOnly = a.split(' ').filter(token => !c.a.split(' ').includes(token)).join(' ')
  await page.setContent(`<style>${result.utilities?.css ?? ''}${css}</style>
    <div id="plain" class="${c.host} ${c.other}">Plain</div>
    <div id="boosted" class="${c.host} ${c.other} ${a}">Boosted</div>
    <div id="hasPlain" class="${c.withHas} ${c.other}"><i class="${c.other}"></i></div>
    <div id="hasBoosted" class="${c.withHas} ${c.other}"><i class="${a}"></i></div>
    <section><i class="${c.other}"></i><div id="nth" class="${c.count} ${c.other} ${c.fallback} ${a}">Second</div></section>
    <div id="negative" class="${c.notTest} ${markerOnly}">Excluded</div>
    <div id="combined" class="${a} ${b} ${c.fallback} ${c.other} ${c.extra}">Combined</div>
    <div id="levels" class="${a2} ${b}">Levels</div>`)
  await expect(page.locator('#plain')).toHaveCSS('color', 'rgb(0, 128, 0)')
  await expect(page.locator('#boosted')).toHaveCSS('color', 'rgb(255, 0, 0)')
  await expect(page.locator('#boosted')).toHaveCSS('background-color', 'rgb(0, 128, 0)')
  await expect(page.locator('#boosted')).toHaveCSS('padding-top', '1px')
  await expect(page.locator('#hasPlain')).toHaveCSS('color', 'rgb(0, 128, 0)')
  await expect(page.locator('#hasBoosted')).toHaveCSS('color', 'rgb(255, 0, 0)')
  await expect(page.locator('#nth')).toHaveCSS('color', 'rgb(255, 0, 0)')
  await expect(page.locator('#negative')).toHaveCSS('display', 'block')
  await expect(page.locator('#combined')).toHaveCSS('border-top-color', 'rgb(255, 0, 0)')
  await expect(page.locator('#levels')).toHaveCSS('outline-color', 'rgb(255, 0, 0)')
})

test('overridden slotted classes retain their target element', async ({ page }) => {
  const compiler = new Compiler()
  compiler.setFile('slot.xcss', 1, '::slotted(.a) { color: red; } ::slotted(.other) { color: green; }')
  compiler.planClassUsage('tsconfig.json', [
    { path: 'tsconfig.json', text: '{"compilerOptions":{"module":"esnext","moduleResolution":"bundler"},"files":["app.ts"]}' },
    { path: 'app.ts', text: "import {override} from '@cross-css/vite'; import {styles} from './slot.xcss'; override(styles.a);" },
  ])
  const { css, classes, overrides } = compiler.compile('slot.xcss').artifacts[0]
  await page.setContent(`<div id="host"><span id="slot" class="${overrides.a['1']} ${classes.other}">Slotted</span></div>`)
  await page.locator('#host').evaluate((element, css) => {
    element.attachShadow({ mode: 'open' }).innerHTML = `<style>${css}</style><slot></slot>`
  }, css)
  await expect(page.locator('#slot')).toHaveCSS('color', 'rgb(255, 0, 0)')
})
