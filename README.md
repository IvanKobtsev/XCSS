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

Existing `var()` calls remain unchanged, while shorthand references in fallbacks are compiled normally.

Arithmetic expressions using `+`, `-`, `*`, or `/` are also wrapped automatically:

```scss
.sidebar {
  width: 100% - --sidebar-width;
  transform: translateX(--offset + 2px);
}

$double-gap: 4px * 2;
```

becomes:

```scss
.sidebar {
  width: calc(100% - var(--sidebar-width));
  transform: translateX(calc(var(--offset) + 2px));
}

$double-gap: calc(4px * 2);
```

Expressions inside `calc()`, `min()`, `max()`, and `clamp()` are already math contexts and are not wrapped again. 
Additive operators must have whitespace on both sides.

Use parentheses when arithmetic is mixed with other whitespace-separated values. XCSS replaces the grouping parentheses with `calc()`:

```scss
/* Ambiguous: compilation fails with a location-aware diagnostic. */
margin: 10px + 2px 20px;

/* Explicit: compiles to `calc(10px + 2px) 20px`. */
margin: (10px + 2px) 20px;
```

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
