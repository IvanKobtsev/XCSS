# XCSS (CrossCSS)

XCSS compiles modern CSS with shorthand values, typed constants, typed custom properties, and
injections into **scoped CSS and class-export mappings**. It preserves native
CSS nesting and runs without Sass.

```xcss
@vars {
  --spacing = 8px;
  --accent: color;
  local --rotation: angle;
  --custom;
}

@injection outlined($width: length = 1px) {
  border: $width solid currentColor;
  @content;
}

.button {
  color: --accent;
  padding: --spacing * 2;
  margin: (10px + 2px) 20px;
  transition: (display, opacity) .3s ease;
  @:outlined(2px) { border-radius: 4px; }
  &:hover { color: blue; }
}
```

Value references become `var()` and arithmetic becomes CSS math. Group arithmetic
when mixing it with whitespace-separated values. Transition property groups are
expanded, and discrete properties receive `allow-discrete` unless `normal` is
specified. Eligible `::before`/`::after` rules receive empty content; use
`content: preserve` to suppress insertion.

## Installation

In an existing Vite 8 project, install the XCSS plugin. Use Node.js 20.19 or later in the 20.x line, or Node.js 22.12 or later.

```console
pnpm add -D @cross-css/vite
```

For npm, use `npm install -D @cross-css/vite`.

Add the plugin to `vite.config.ts` (or `vite.config.js`).

```ts
import { defineConfig } from 'vite'
import xcss from '@cross-css/vite'
export default defineConfig({ plugins: [xcss()] })
```

```ts
import { styles } from './button.xcss'
import inlineCss from './button.xcss?inline'
import stylesheetUrl from './button.xcss?url'
```

The named `styles` export maps local class names to generated class strings.
Side-effect imports also work. Inline imports return processed CSS
without injecting it; URL imports return a stylesheet URL. XCSS owns scoping;
Vite handles plain CSS delivery, extraction, assets, and native CSS imports.

### TypeScript

Add `@cross-css/vite/client` to the `compilerOptions.types` array in the config
that includes your app files. Keep existing entries such as `vite/client`.
You can also put the following reference in an environment declaration file
such as `src/vite-env.d.ts`.

```ts
/// <reference types="vite/client" />
/// <reference types="@cross-css/vite/client" />
```

The XCSS declarations cover ordinary `.xcss` imports. If your project
type-checks `?inline` and `?url` imports, add these declarations to that same
environment file.

```ts
declare module '*.xcss?inline' {
  const css: string
  export default css
}

declare module '*.xcss?url' {
  const url: string
  export default url
}
```

### React JSX with `xClass` and `xVars`

These props use the XCSS automatic JSX runtime and work on intrinsic elements
such as `<button>`. The runtime supports React 18.3 and React 19. In a project
using `@vitejs/plugin-react`, configure the same JSX import source in Vite and
TypeScript. Keep XCSS before plugins that rewrite JSX.

```ts
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import xcss from '@cross-css/vite'

export default defineConfig({
  plugins: [
    xcss({ tsconfig: 'tsconfig.app.json' }),
    react({ jsxImportSource: '@cross-css/vite/react' }),
  ],
})
```

In `tsconfig.app.json`, merge these options into your existing configuration.

```json
{
  "compilerOptions": {
    "jsx": "react-jsx",
    "jsxImportSource": "@cross-css/vite/react",
    "types": ["vite/client", "@cross-css/vite/client"]
  }
}
```

Vite uses `tsconfig.json` for XCSS analysis when you call `xcss()`. If your
root config only references other configs, pass the app config as shown above.
It must include the JSX files that use `xClass` and the component call sites
XCSS needs to trace. For JavaScript JSX files, set `allowJs: true` and include
the `.jsx` files in that config. If you use Vite without the React plugin,
set `oxc.jsx` to
`{ runtime: 'automatic', importSource: '@cross-css/vite/react' }` in the Vite config.

Create `Button.xcss` and import its named exports.

```xcss
@vars {
  @provided --button-width: length = 0px;
}

@variants Tone: .primary, .secondary;
.button { width: --button-width; }
.primary { color: navy; }
.secondary { color: gray; }
```

```tsx
import { styles, cssVars, type Variants } from './Button.xcss'
import { px } from '@cross-css/vite/units'

export function Button({ tone, width }: { tone: Variants<'Tone'>; width: number }) {
  return (
    <button
      className="global-button"
      xClass={[styles.button, tone]}
      xVars={{ [cssVars.buttonWidth]: px(width) }}
    >
      Continue
    </button>
  )
}
```

Use an inline array or a fixed readonly tuple for `xClass`. Later slots boost
eligible stylesheet-root selectors. A conditional value such as
`active && styles.active` keeps its slot when false. Use ordinary `className`
for global class names.

With `noUncheckedIndexedAccess` enabled, TypeScript treats keys in the
bundled `cssVars` declaration as potentially absent. For a variable declared
in the stylesheet, use `[cssVars.buttonWidth!]` as the computed key.
`xcss check` verifies that the export exists.

### Check your setup

Add an npm script that points to the same app config used by the plugin.

```json
{
  "scripts": {
    "check:xcss": "xcss check --project tsconfig.app.json"
  }
}
```

Run `pnpm check:xcss` alongside your normal TypeScript check. The bundled
TypeScript declarations make stylesheet imports and JSX props available.
`xcss check` validates actual exported names, variant values, `xClass` slots,
and `xVars` keys and values against your stylesheets.

## Variables and injections

An initial value infers the registration type. An explicit annotation validates
it. A typed declaration without an initial value gets a documented type default
(for example transparent for color and 0px for length). An untyped declaration
without an initial value uses wildcard syntax. `local` means `inherits: false`.
Registration values must meet CSS computational-independence requirements.
Frequency and flex use wildcard browser registration with static XCSS checking.
Generated custom-property names are available in Rust/Node artifact metadata.

Injection imports use `@import ns from "./library.xcss"` or
`@import { outlined } from "./library.xcss"`. They import definitions, never the
library's unrelated selectors. Local definitions win; otherwise globally unique
injections can be called without imports. Expansion-generated classes belong to
the caller. Definition-owned variables and asset URLs retain their origins.

Compile-time constants use `$name = value;` or `$name: type = value;`. They are
immutable, visible after declaration, and scoped to their source block and its
descendants. Redeclaration and shadowing are errors. Values are substituted into
CSS; runtime expressions such as `var()` remain dynamic:

```xcss
$header-height = 56px;
$height: length = $header-height + --my-height / 2;
$gradient = linear-gradient(to right, #000 --start, #fff (--end + --offset));

.header {
  height: $height;
  background: $gradient;
}
```

Types are inferred from values and typed references. Supply an annotation when
the type is unknown. Constants work in values, injection arguments/defaults,
and media/supports/container conditions. Injection-local constants may depend on
parameters. Constants are file-local and have no import/export syntax.

Classes, IDs, and animation names are scoped. Functional and nonfunctional
`:local`/`:global` switches are supported. Use `@util` to share styles between
classes. XCSS rejects `composes` declarations and passes `@raw` CSS through.
XCSS does not support mutable Sass variables, loops, interpolation, custom
compile-time functions, or ICSS value import/export syntax.

## CLI and Rust/Node APIs

Run `xcss check` from the project directory to check stylesheets. See
[Check your setup](#check-your-setup) for project-aware checks.
If TypeScript setup fails, the command prints a warning and still checks the
stylesheets.

```console
cargo run -p xcss-cli -- compile styles/button.xcss --output dist/button.css --root .
```

The CLI writes scoped `.css`, adjacent `.css.map` and `.exports.json`.
Do not name output `.module.css`: names are
already scoped. Compilation errors preserve previous artifacts.

The Rust `xcss-core` facade exposes project inspection, compilation and the
snapshot-based engine. `@cross-css/native` exposes a reusable `Compiler` class;
see its [API examples](packages/native/README.md).

## Development

```console
corepack pnpm install
corepack pnpm test:engine
corepack pnpm check:architecture
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
corepack pnpm check
corepack pnpm test
corepack pnpm --filter xcss-vite-fixture exec playwright install chromium
corepack pnpm --filter xcss-vite-fixture test:browser
```

See [architecture](docs/architecture.md), [language](docs/language.md),
[generation](docs/generation.md), and [release instructions](RELEASING.md).
The compiler is early-stage; generated formatting and public APIs are evolving.
