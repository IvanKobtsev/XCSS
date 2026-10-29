# XCSS (CrossCSS)

XCSS compiles modern CSS with shorthand values, typed custom properties, and
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

## Vite

```console
pnpm add -D @cross-css/vite-plugin
```

```ts
import { defineConfig } from 'vite'
import xcss from '@cross-css/vite-plugin'
export default defineConfig({ plugins: [xcss()] })
```

```ts
import styles from './button.xcss'
import inlineCss from './button.xcss?inline'
import stylesheetUrl from './button.xcss?url'
```

The default export maps local class names to generated class strings, including
compositions. Side-effect imports also work. Inline imports return processed CSS
without injecting it; URL imports return a stylesheet URL. XCSS owns scoping;
Vite handles plain CSS delivery, extraction, assets, and native CSS imports.

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

Classes, IDs, and animation names are scoped. Functional and nonfunctional
`:local`/`:global` switches are supported. Class composition supports project
XCSS modules and global names. External CSS Modules composition needs a future
adapter. XCSS does not yet support Sass variables, loops, interpolation, custom
compile-time functions, or ICSS value import/export syntax.

## CLI and Rust/Node APIs

```console
cargo run -p xcss-cli -- compile styles/button.xcss --output dist/button.css --root .
```

The CLI writes scoped `.css`, adjacent `.css.map` and `.exports.json`, and
composition dependency artifacts. Do not name output `.module.css`: names are
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
