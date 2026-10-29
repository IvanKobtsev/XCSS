# @cross-css/vite-plugin

Compile `.xcss` into scoped CSS and JavaScript class exports with Vite 8.

```ts
import { defineConfig } from 'vite'
import xcss from '@cross-css/vite-plugin'
export default defineConfig({ plugins: [xcss()] })
```

```ts
import styles from './button.xcss'       // inject/extract CSS, export class names
import './reset.xcss'                   // side-effect styles
import text from './button.xcss?inline' // processed CSS, no injection
import href from './button.xcss?url'    // stylesheet URL
```

XCSS scopes names before handing plain virtual CSS to Vite. Vite handles native
CSS imports, asset URLs, development styling, production extraction and targeting.
There is no Sass dependency, generated partial, or second CSS Modules pass.
Compositions use XCSS's own class-export dictionary. Inline/URL stylesheets
include composition dependencies. Unsupported or conflicting query modes fail
explicitly. Aliases are resolved by Vite.

A project maintains one native analysis host. Edits update source revisions;
additions/removals rebuild the global injection index. Active XCSS wrappers and
stylesheets are invalidated together because unimported global injection calls
can be affected. Native source maps point to original XCSS, including imported
injection definitions and caller-owned arguments/content.
