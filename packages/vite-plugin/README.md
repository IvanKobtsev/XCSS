# `@cross-css/vite-plugin`

Compile `.cross.css` and `.cross.scss` files with XCSS before Vite's CSS pipeline.

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

Place XCSS first among plugins using `enforce: 'pre'`. Every Cross file is exposed to Vite as a CSS Module. External stylesheet directives (`@import`, `@use`, and `@forward`) are reserved for XCSS's future import system.
