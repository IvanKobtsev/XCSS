import { test } from 'node:test'
import assert from 'node:assert/strict'
import { Compiler, LanguageService } from '../index.js'

const config = JSON.stringify({ compilerOptions: { strict: true, jsx: 'preserve', module: 'esnext', moduleResolution: 'bundler' }, files: ['app.tsx', 'helper.ts'] })
function setup(source, css = '.a { color: red; } .b { color: blue; }', helper = "export { override as boost } from '@cross-css/vite';") {
  const compiler = new Compiler()
  compiler.setFile('a.xcss', 1, css)
  const plan = compiler.planClassUsage('tsconfig.json', [
    { path: 'tsconfig.json', text: config }, { path: 'helper.ts', text: helper },
    { path: 'app.tsx', text: `import {override} from '@cross-css/vite'; import {styles} from './a.xcss'; ${source}` },
  ])
  return { compiler, plan }
}

test('override plans aliases, non-JSX calls, forwarding, and only requested levels', () => {
  const { compiler, plan } = setup(`
    import {boost} from './helper';
    import * as helpers from '@cross-css/vite';
    type ClassName = string & { readonly __xcssClassName__: true };
    function Button({className}: {className: ClassName}) { return <button xClass={override(className)} />; }
    function forward(value: ClassName): ClassName { return value; }
    const element = <Button className={styles.a}/>;
    const alias: (value: ClassName, level?: number) => ClassName = (boost);
    const value = alias(forward(styles.b), 5);
    helpers.override(styles.a);
  `)
  assert.deepEqual(plan.diagnostics, [])
  assert.deepEqual([...new Set(plan.sites.map(s => s.level))].sort(), ['1', '5'])
  const artifact = compiler.compile('a.xcss').artifacts[0]
  assert.deepEqual(Object.keys(artifact.overrides.a), ['1'])
  assert.deepEqual(Object.keys(artifact.overrides.b), ['5'])
  assert.equal(artifact.overrides.b['5'], 'b_000 b_000_override_5')
  assert(!artifact.css.includes('override_2'))
  compiler.clearClassUsage()
  assert(!compiler.compile('a.xcss').artifacts[0].css.includes('override_'))
})

test('invalid levels, unknown origins, repeated overrides and escaping helpers are errors', () => {
  const { plan } = setup(`
    declare const level: number; declare function getLevel(): number;
    override(styles.a, level); override(styles.a, getLevel()); override(styles.a, 1.5);
    override(styles.a, -1); override(styles.a, 0); override(styles.a, undefined);
    override(styles.a, 1 + 1); override(styles.a, 2 as const);
    const prior = override(styles.a); override(prior); override(override(styles.a, 2));
    declare const unknown: string & {readonly __xcssClassName__: true}; override(unknown);
    [styles.a].map(override);
    declare function getHelper(): typeof override;
    getHelper()(styles.a);
    declare const object: { readonly boost: typeof override };
    object.boost(styles.a);
  `)
  const codes = plan.diagnostics.map(d => d.code)
  assert.equal(codes.filter(c => c === 'XC-010').length, 8)
  assert.equal(codes.filter(c => c === 'XC-012').length, 2)
  assert(codes.includes('XC-011'))
  assert(codes.includes('XC-013'))
})

test('module and contributed utility classes receive markers without renumbering base names', () => {
  const css = '@util padded { padding: 8px; } .a { @:padded; color: red; } :global(.a_000_override_2) {} .__xcssOverrideHelper__ {}'
  const { compiler, plan } = setup('const untouched = styles.__xcssOverrideHelper__; const value = override(styles.a, 2);', css)
  assert.deepEqual(plan.diagnostics, [])
  const result = compiler.compile('a.xcss')
  assert.equal(result.success, true)
  const artifact = result.artifacts[0]
  assert.equal(artifact.classes.a, 'a_000 _util_padded_000')
  assert(artifact.overrides.a['2'].includes('a_000_override_2_1'))
  assert(artifact.overrides.a['2'].includes('_util_padded_000_override_2'))
  assert(result.utilities.css.includes('_util_padded_000_override_2:is(*, #_#_)'))
})

test('pseudo-class alternatives preserve original membership and negative selectors are unboosted', () => {
  const { compiler, plan } = setup('override(styles.a, 2);', '.a {} .host:is(.a, .b) { color: red; } :nth-child(2 of .a, .b) { color: blue; } :not(.a) {opacity: .5} .raw { @raw { &.untouched { color: red; } } }')
  assert.deepEqual(plan.diagnostics, [])
  const artifact = compiler.compile('a.xcss').artifacts[0]
  assert(artifact.css.includes(':where(:is(.a_000_override_2))'))
  assert(artifact.css.includes(':where(:nth-child(2 of .a_000, .b_000):where(.a_000_override_2))'))
  assert(artifact.css.includes(':not(:is(.a_000, .a_000_override_2))'))
  assert(artifact.css.includes('&.untouched'))
})

test('JavaScript helper validation remains active with checkJs disabled', () => {
  const compiler = new Compiler()
  compiler.setFile('a.xcss', 1, '.a { color: red; }')
  const plan = compiler.planClassUsage('tsconfig.json', [
    { path: 'tsconfig.json', text: '{"compilerOptions":{"allowJs":true,"checkJs":false,"module":"esnext","moduleResolution":"bundler"},"files":["app.js"]}' },
    { path: 'app.js', text: "import {override} from '@cross-css/vite'; import {styles} from './a.xcss'; const level=2; override(styles.a); override(styles.a, level); override(styles.missing);" },
  ])
  assert(plan.diagnostics.some(d => d.code === 'XC-010'))
  assert(plan.diagnostics.some(d => d.code === 'XC-001'))
  assert.equal(plan.sites.filter(s => s.level === '1').length, 1)
})

test('editor diagnostics validate unsaved helper calls and update after edits', () => {
  const service = new LanguageService()
  service.setTypescriptProject('tsconfig.json')
  service.setTypescriptFile('tsconfig.json', 1, '{"compilerOptions":{"module":"esnext","moduleResolution":"bundler"},"files":["app.ts"]}')
  service.setFile('a.xcss', 1, '.a { color: red; }')
  const source = "import {override} from '@cross-css/vite'; import {styles} from './a.xcss'; const level=2; override(styles.a, level);"
  service.setTypescriptFile('app.ts', 1, source)
  const error = service.document('app.ts').diagnostics.find(d => d.code === 'XC-010')
  assert(error)
  assert.equal(error.location.range.start.character, source.lastIndexOf('level'))
  service.setTypescriptFile('app.ts', 2, source.replace('override(styles.a, level)', 'override(styles.a, 2)'))
  assert.deepEqual(service.document('app.ts').diagnostics, [])
})
