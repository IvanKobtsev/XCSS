# @cross-css/vite

Compile `.xcss` into scoped CSS and JavaScript class exports with Vite 8.

```ts
import { defineConfig } from 'vite'
import xcss from '@cross-css/vite/react-plugin'
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
`"tsconfig": "./tsconfig.app.json"` in `xcss.config.json`. Vite aliases do not apply to these
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

Keep `xcss()` in the Vite plugins and load the React prop types in your app's
TypeScript configuration. Keep your existing JSX runtime configuration.

```json
{
  "compilerOptions": {
    "jsx": "react-jsx",
    "types": ["@cross-css/vite/client"]
  }
}
```

Load the prop augmentation in a declaration file included by your project, such as
`react-env.d.ts`:

```ts
import type {} from '@cross-css/vite/react'
```

XCSS lowers affected native JSX elements before React's JSX compiler, using
React's public `createElement` API. No `jsxImportSource` override or React plugin
configuration is needed. Attribute expressions and spreads evaluate once with
React's automatic JSX ordering and key precedence; later ordinary props win.
Custom components keep their original JSX and props.

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
compose classes with clsx semantics: strings, numbers, nested or dynamic arrays,
conditional values, and object maps are supported. Falsy values contribute nothing.
Typed `Variants<'Group'>` leaves still resolve to their scoped classes; ordinary
strings remain global class names. No fixed tuple or runtime class allowlist is
required.

```tsx
import type { ClassName } from '@cross-css/vite/types'
import { styles } from './Box.xcss'

export function Box({ active, className }: { active: boolean; className?: ClassName }) {
  return <div className="global" xClass={[styles.box, active && styles.active, className]} />
}
```

The runtime appends the contributed classes to `className`. Array positions do
not affect specificity or CSS precedence; normal CSS cascade rules apply. Variant
entries resolve labels to the classes declared in their group. Nested XCSS rules
compile to flat CSS, with independent specificity for each parent-list branch.
Vite uses the root `tsconfig.json` to find usages. Set
`"tsconfig": "./tsconfig.app.json"` in `xcss.config.json` for a split config, and use the same
config with `xcss check --project`.

Use `@util` for shared styles. XCSS rejects `composes` declarations and passes
CSS within `@raw` through to the stylesheet.

Shared class types are available from `@cross-css/vite/types` for both React and
Solid JS:

```ts
import type { ClassName, XClass } from '@cross-css/vite/types'
```

Breaking change: `ClassName` is no longer exported from `@cross-css/vite/react`
or `@cross-css/vite/solidjs`. Update those type imports to use `/types`.
Framework-specific JSX augmentations still load from `/react` and `/solidjs`.

## Explicit specificity overrides

Import `override` from `@cross-css/vite` to opt a class into additional specificity:

```tsx
import { override } from '@cross-css/vite'
import type { ClassName } from '@cross-css/vite/types'

function Button(props: { className: ClassName }) {
  return <button xClass={[styles.button, override(props.className)]} />
}

const highlighted = override(styles.highlighted, 2)
```

The default level is 1. A supplied level must be a positive integer numeric
literal; variables, calls, arithmetic, assertions, zero, and negative or fractional
values are compile-time errors. Parenthesized literals work. There is no preset
level ceiling or pregenerated sequence of levels; excessive generated output
fails with a resource-limit diagnostic.

The compiler must trace each input to finite XCSS class exports, including through
component props and function forwarding. An unconstrained ClassName with unknown
external callers is an error. The helper works in project JavaScript/TypeScript
files outside JSX as well as in xClass and className. Applying it to an already
overridden value is an error, including across component boundaries. Untransformed
calls and unexpected runtime inputs throw.

Original classes remain on the element alongside class-specific override markers.
Contributed @util classes receive markers too. Marked selector occurrences gain
the requested number of ID selectors, subject to native CSS function and cascade
rules: :where() still has zero specificity, and cascade layers and !important
retain their normal precedence. Positive functional alternatives use marker
guards so an unrelated branch cannot gain specificity. Inside :not(), classes
become an unboosted :is(original, markers) union.

Only traced class/level pairs are emitted. Rebuild CSS and its JavaScript consumers
together. With the CLI, use compile --project to collect requests from TypeScript;
JavaScript helper calls still require the Vite transform.

## Shared utilities

`@util` class compositions automatically load one shared utility stylesheet.
Development updates its active usages through HMR; production includes utilities
from eager and lazy modules in one CSS asset. Selector-based calls and
`as injection` remain in the consuming module's CSS, inside `@layer utils`.
`?inline` and `?url` requests include their utilities in their bundled stylesheet.


## Project configuration

All project settings live in root `xcss.config.json`; call the Vite plugin as `xcss()`.

```json
{
  "suffixLength": 3,
  "tsconfig": "./tsconfig.app.json"
}
```

Both fields are optional. The defaults are three characters and `./tsconfig.json`.
Suffix length must be an integer from 1 through 16. Scoped names use per-name,
zero-padded base-36 counters (`button_000`, `button_001`, `card_000`). Allocation
is deterministic across the complete loaded project. Adding or removing definitions
may renumber names: rebuild CSS and its consumers together. Separate projects do
not coordinate names. Exhausting the suffix space produces an error; increase
`suffixLength` to provide more capacity. Configuration changes restart Vite.

This replaces the previous hashed output names and the plugin's `tsconfig` option.
The existing `cross-figma` section can coexist with these settings. CLI `--project`
remains available as a one-off TypeScript project override.

### SolidJS 2

Use Node.js 22.12 or later and install solid-js 2, @solidjs/web 2, and
@solidjs/vite-plugin 3 alongside Vite 8. Configure XCSS before Solid:

```ts
import { defineConfig } from 'vite'
import solid from '@solidjs/vite-plugin'
import xcss from '@cross-css/vite/solidjs-plugin'
export default defineConfig({ plugins: [xcss(), solid()] })
```

Keep TypeScript JSX preserved with jsxImportSource set to @solidjs/web.
Add @cross-css/vite/solidjs to compilerOptions.types (alongside vite/client
and @cross-css/vite/client) for xClass and xVars declarations.
Intrinsic elements accept reactive xClass and xVars expressions with the same
clsx composition, typed variants, and explicit overrides as React. Existing class bindings are
preserved; xVars overrides matching style properties. Component props are
passed through for forwarding to intrinsic elements.

The package root no longer exports a Vite plugin. React applications should
import their plugin from @cross-css/vite/react-plugin.
