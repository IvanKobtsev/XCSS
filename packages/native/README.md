# @cross-css/native

Native Node.js bindings for XCSS parsing, analysis, and scoped CSS generation.
Consumers do not need Rust or Sass installed; platform packages provide binaries.

```js
import { Compiler, inspect } from '@cross-css/native'
const compiler = new Compiler()
compiler.setFile('button.xcss', 1, '.button { padding: --space * 2; }')
const result = compiler.compile('button.xcss')
if (!result.success) throw new Error(JSON.stringify(result.diagnostics))
const artifact = result.artifacts.find(item => item.moduleId === 'button.xcss')
console.log(artifact.css, artifact.classes, artifact.sourceMap)
```

Use normalized, project-relative module IDs and increasing revisions. File updates
reuse other parse trees. `removeFile(id, revision)` retains the deletion revision
so stale updates cannot resurrect files. Host adapters load files and resolve
imports; the native compiler performs no filesystem access.

`inspect(source).imports` returns injection/composition specifiers. Supply
`setResolution(importer, specifier, targetOrNull)` for each. All project files
should be loaded to support globally unique injection resolution.

`compile(entry, minify?, bundle?)` returns success, artifacts, and diagnostics.
Default artifacts are per-module scoped CSS with class mappings, custom-property
names, original-source maps, native imports, composition stylesheet dependencies,
and source dependencies. Bundling combines only composition dependencies and
rebases their assets to the entry. Injection dependencies contribute definitions
only. Diagnostics carry stable codes, severity, module ID, byte span, related
locations, and one-based UTF-16 line/column positions. Errors return no artifacts.

`index.d.ts` is generated from napi-rs binding metadata during the native build.
# Editor queries

Version 0.3.0 also exports `LanguageService` and `languageServiceCapabilities()`.
The service accepts the same versioned in-memory updates as `Compiler`, with file
URIs supported as identities. `document(uri)` returns diagnostics and semantic
tokens; `definition`, `references`, and `hover` accept zero-based UTF-16 positions.
These APIs do not compile CSS or read files. See the repository's
[language-service contract](https://github.com/IvanKobtsev/XCSS/blob/main/docs/language-service.md).



## CSS declaration validation inputs (API v2, native 0.3.1)

`LanguageService.cssDeclarations(uri)` returns `{ revision, declarations }` from
the same analysis snapshot as other editor queries, without compiling CSS.
Each declaration contains `property`, `propertyRange`, `valueRange`, `context`,
`checkable`, and ordered `parts`. Ranges are zero-based UTF-16 positions in the
original source. Context is stylesheet, rule, injection, keyframes, or at-rule.
Each part contains a `range` and either literal `text` or a resolved `valueType`;
the other field is absent. Literal gaps, whitespace, comments, and CSS function
syntax are retained. Resolved parameters and declared custom-property references
use the existing analyzer's types, never their initializer or call-site argument.
Arithmetic with a known analysis result type is exposed as a single typed part.

Unknown references, native var(), recovery regions, transition groups and other
unsupported transformations make values uncheckable; property names remain
available. Parts on uncheckable declarations are incomplete and must not be
validated. XCSS directives, variable registrations, and direct declarations in
at-rule bodies (descriptors) are excluded. Rules nested inside at-rules remain
eligible. Consumers may substitute representative CSS values for typed parts;
this is a broad compatibility check, not proof for all values of that type.
Changes to any file or resolution invalidate the snapshot revision.
