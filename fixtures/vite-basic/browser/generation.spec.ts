import { test,expect } from '@playwright/test'
test.beforeEach(async({page})=>{await page.goto('/')})
test('registered defaults and non-inheritance work in a browser',async({page})=>{
 await expect(page.locator('#child')).toHaveCSS('opacity','0')
 await expect(page.locator('#parent')).toHaveCSS('color','rgba(0, 0, 0, 0)')
})
test('native nesting, composed exports and scoped keyframes work',async({page})=>{
 await page.locator('#child').hover();await expect(page.locator('#child')).toHaveCSS('color','rgb(0, 0, 255)')
 await expect(page.locator('#composed')).toHaveCSS('background-color','rgb(255, 0, 0)')
 expect((await page.locator('#composed').getAttribute('class'))!.split(' ')).toHaveLength(2)
 await expect(page.locator('#animated')).toHaveCSS('animation-name',/^xcss_.*_pulse$/)
})
test('stylesheet URLs serve CSS and inline imports stay text',async({page})=>{
 const url=await page.locator('#app').getAttribute('data-stylesheet');expect(url).toBeTruthy()
 const response=await page.request.get(url!);expect(response.ok()).toBe(true);expect(response.headers()['content-type']).toContain('text/css');expect(await response.text()).toContain('var(--space)')
 expect(await page.locator('#app').getAttribute('data-inline')).toContain('var(--space)')
})
