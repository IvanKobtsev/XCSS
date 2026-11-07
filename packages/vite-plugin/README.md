# @cross-css/vite

Compile `.xcss` into scoped CSS and JavaScript class exports with Vite 8.

```ts
import { defineConfig } from 'vite'
import xcss from '@cross-css/vite'
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
import styles, { type Variants } from './Button.xcss'
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
installing dependencies. XCSS uses tsrs for this check while your project's
TypeScript checker uses the permissive declarations. Groups belong to their XCSS module and list
exported local classes, without requiring a corresponding base selector.
Vite compilation does not automatically run TS project checking.

Variant values are literal names such as `primary`. This feature checks them;
it does not translate them to scoped classes or implement an `xClass` attribute.
Use `styles.primary` when applying the existing scoped class at runtime.

A project maintains one native analysis host. Edits update source revisions;
additions/removals rebuild the global injection index. Active XCSS wrappers and
stylesheets are invalidated together because unimported global injection calls
can be affected. Native source maps point to original XCSS, including imported
injection definitions and caller-owned arguments/content.

## Shared utilities

`@util` class compositions automatically load one shared utility stylesheet.
Development updates its active usages through HMR; production includes utilities
from eager and lazy modules in one CSS asset. Selector-based calls and
`as injection` remain in the consuming module's CSS, inside `@layer utils`.
`?inline` and `?url` requests include their utilities in their bundled stylesheet.
