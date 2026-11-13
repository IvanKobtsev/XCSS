# @cross-css/vite

Compile `.xcss` into scoped CSS and JavaScript class exports with Vite 8.

```ts
import { defineConfig } from 'vite'
import xcss from '@cross-css/vite'
export default defineConfig({ plugins: [xcss()] })
```

```ts
import { styles } from './button.xcss'       // inject/extract CSS, export class names
import './reset.xcss'                   // side-effect styles
import text from './button.xcss?inline' // processed CSS, no injection
import href from './button.xcss?url'    // stylesheet URL
```

XCSS scopes names before handing plain virtual CSS to Vite. Vite handles native
CSS imports, asset URLs, development styling, production extraction and targeting.
There is no Sass dependency, generated partial, or second CSS Modules pass.
Unsupported or conflicting query modes fail
explicitly. Imports inside XCSS and aliased `url()` assets resolve from the selected
project's `compilerOptions.paths`, using root `tsconfig.json` by default or
`xcss({ tsconfig: 'tsconfig.app.json' })`. Vite aliases do not apply to these
references. JavaScript/TypeScript imports retain Vite's normal resolution.

`paths` supports exact names, wildcard patterns, ordered target fallbacks,
`extends`, and `baseUrl`. Declare `"@/*": ["./src/*"]` explicitly to use `@/`;
there is no implicit root alias. See [language documentation](../../docs/language.md#path-aliases)
for stylesheet and asset examples.

## TypeScript and variant groups

Add `"@cross-css/vite/client"` to your tsconfig's `compilerOptions.types`,
or add `/// <reference types="@cross-css/vite/client" />` to an environment
declaration file. This declaration entry is included in this package.
It lets ordinary TypeScript accept XCSS imports and the `Variants` type:

```xcss
@variants Button: .primary, .secondary;
.button { color: black; }
.primary { color: blue; }
.secondary { color: gray; }
```

```ts
import { styles, type Variants } from './Button.xcss'
interface ButtonProps { variant: Variants<'Button'> }
```

The package provides the `xcss` command. Add a script to validate exact class
keys and variant values:

```json
{
  "scripts": {
    "check:xcss": "xcss check --project tsconfig.app.json"
  }
}
```

Use the config that includes your application's TypeScript files, then run
`yarn check:xcss` (or `npm run check:xcss`). CI can run the same script after
installing dependencies. Run `xcss check` for stylesheet checks alone. A
TypeScript setup failure prints a warning while stylesheet checks continue.
XCSS uses tsrs for this check while your project's
TypeScript checker uses the permissive declarations. Groups belong to their XCSS module and list
exported local classes, without requiring a corresponding base selector.
Vite compilation does not automatically run TS project checking.

Variant values are literal names such as `primary`. Use `styles.primary` to
apply the corresponding scoped class at runtime.

A project maintains one native analysis host. Edits update source revisions;
additions/removals rebuild the global injection index. Active XCSS wrappers and
stylesheets are invalidated together because unimported global injection calls
can be affected. Native source maps point to original XCSS, including imported
injection definitions and caller-owned arguments/content.

## Provided variables in React

Keep `xcss()` in the Vite plugins and select the XCSS automatic JSX runtime in
your app's TypeScript configuration.

```json
{
  "compilerOptions": {
    "jsx": "react-jsx",
    "jsxImportSource": "@cross-css/vite/react",
    "types": ["@cross-css/vite/client"]
  }
}
```

When using `@vitejs/plugin-react`, pass
`react({ jsxImportSource: '@cross-css/vite/react' })`. For a JavaScript Vite
project without that plugin, set
`oxc.jsx: { runtime: 'automatic', importSource: '@cross-css/vite/react' }`.

```xcss
@vars {
  @provided --provided-width = 0px;
}
.box { width: --provided-width; }
```

```tsx
import { styles, cssVars } from './Box.xcss'
import { px } from '@cross-css/vite/units'

export function Box({ width }: { width: number }) {
  return <div className={styles.box} style={{ height: '200px' }}
    xVars={{ [cssVars.providedWidth]: px(width) }} />
}
```

The runtime merges `xVars` into `style` on intrinsic elements, including when
`xVars` comes from a JSX spread. If both props set the same custom property,
`xVars` takes precedence.

Run `xcss check --project tsconfig.app.json` to check that each key resolves to a
provided variable and that its value matches the XCSS type. Unit helpers such as
`px`, `percentage`, and `vh` accept a finite number or numeric string and
preserve the unit in the return type. Compatible literals work directly. For
reusable maps, keep literal values narrow with `as const` or use a unit helper. Use an
explicit `CssValue<'length'>` assertion for a dynamic CSS string whose type you
know, such as a constructed `calc()` expression.

## Composing classes in React

With the XCSS JSX runtime configured, use `xClass` on intrinsic elements to
compose scoped class names. A single class uses slot zero. For multiple slots,
pass an array literal or a fixed readonly tuple of
named `styles` exports, values typed as `Variants<'Group'>`, and conditional
slots. Booleans, `null`, and `undefined` leave the slot empty. Use `className`
for global classes.

```tsx
import type { ClassName } from '@cross-css/vite/react'
import { styles } from './Box.xcss'

export function Box({ active, className }: { active: boolean; className?: ClassName }) {
  return <div className="global" xClass={[styles.box, active && styles.active, className]} />
}
```

The runtime appends the contributed classes and positional aliases to
`className`. Later slots add more specificity to eligible stylesheet-root
rules. Variant slots resolve labels to the classes declared in their group.
Vite uses the root `tsconfig.json` to find usages. Set
`xcss({ tsconfig: 'tsconfig.app.json' })` for a split config, and use the same
config with `xcss check --project`.

Use `@util` for shared styles. XCSS rejects `composes` declarations and passes
CSS within `@raw` through to the stylesheet.

## Shared utilities

`@util` class compositions automatically load one shared utility stylesheet.
Development updates its active usages through HMR; production includes utilities
from eager and lazy modules in one CSS asset. Selector-based calls and
`as injection` remain in the consuming module's CSS, inside `@layer utils`.
`?inline` and `?url` requests include their utilities in their bundled stylesheet.
