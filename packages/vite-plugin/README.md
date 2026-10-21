# `@cross-css/vite-plugin`

Compile `.xcss` files as SCSS-backed CSS Modules before Vite's CSS pipeline.

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

Place XCSS first among plugins using `enforce: 'pre'`. Every `.xcss` file is exposed as a `.module.scss` module. XCSS injection imports are resolved at compile time and never import the source stylesheet's selectors.
