# XCSS (CrossCSS)

XCSS is a parser-backed Rust compiler that makes you cross out all the boilerplate from your CSS. It compiles `.cross.css` 
and `.cross.scss` files into CSS Module files while preserving the original formatting and comments.

```scss
.button {
  color: var(--primary-color);
  padding: calc(var(--spacing) * 2);
}
```

with XCSS can be written as simply:

```scss
.button {
  color: --primary-color;
  padding: --spacing * 2;
}
```

Use parentheses when arithmetic is mixed with other whitespace-separated values. XCSS replaces the grouping parentheses with `calc()`:

```scss
/* Ambiguous: compilation fails with a location-aware diagnostic. */
margin: 10px + 2px 20px;

/* Explicit: compiles to `calc(10px + 2px) 20px`. */
margin: (10px + 2px) 20px;
```

Transition properties can share the same timing values by grouping them in the
property-first position:

```scss
/* Compiles to: color .5s ease, --theme .5s ease, width .3s */
transition: (color, --theme) .5s ease, width .3s;

/* Compiles to: --theme var(--duration) var(--easing), width var(--duration) var(--easing) */
transition: (--theme, width) --duration --easing;
```

XCSS automatically adds `allow-discrete` when a `transition` item targets `display`,
`content-visibility`, or `overlay`. An explicit `normal` opts out of this behavior.
Custom property names in a transition property group remain bare, while custom values
used for duration, easing, or delay are wrapped in `var()`. A single ungrouped custom
property is also supported when it is unambiguous, such as `--theme .3s ease`.
Ambiguous items such as `--property --duration ease` fail compilation; write the
property explicitly as `(--property) --duration ease` instead. Custom property names
in `transition-property` declarations always remain bare.

XCSS is in very early development and haven't been thoroughly tested yet. Expect valid XCSS syntax to fail on compilation.

## Vite plugin

The workspace includes `@cross-css/vite-plugin` for Vite 8. Add it first among user `pre` plugins so XCSS compiles direct 
`.cross.css` and `.cross.scss` imports before Vite's CSS, CSS Modules, PostCSS, and Sass processing:

```console
pnpm add -D @cross-css/vite-plugin
```

```ts
import { defineConfig } from 'vite'
import xcss from '@cross-css/vite-plugin'

export default defineConfig({
  plugins: [xcss()],
})
```

Then import Cross files directly from JavaScript or TypeScript:

```ts
import styles from './button.cross.scss';
```

Every Cross file is treated as a CSS Module. The plugin preserves Vite query parameters and keeps its virtual `.module.css` 
or `.module.scss` file beside the source path, so relative `url(...)` references continue to resolve normally.

External stylesheet directives (`@import`, `@use`, and `@forward`) are reserved for XCSS's future import system and 
currently produce a compiler diagnostic. Alias resolution still occurs before XCSS; 
among multiple user `pre` plugins, keep XCSS first when it must run first.

## Development

Build and test the full workspace with:

```console
corepack pnpm install
corepack pnpm build
corepack pnpm test
```

Published releases use platform-specific native packages selected automatically by `@cross-css/native`; consumers do not need Rust installed. Maintainer release instructions are in [RELEASING.md](RELEASING.md).

## CLI usage

```console
cargo run -p xcss-cli -- compile styles/button.cross.css --output styles/button.module.css
```

SCSS inputs must use a matching SCSS output:

```console
cargo run -p xcss-cli -- compile styles/button.cross.scss --output styles/button.module.scss
```

The reusable `xcss-core` crate exposes the in-memory `transform` API used by the napi-rs binding and Vite plugin. The CLI performs file I/O only after parsing and transformation succeed.
