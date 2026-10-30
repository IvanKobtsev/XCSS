# Language service

`xcss-language-service` provides editor queries without generation, filesystem
access, or a protocol dependency. Hosts provide normalized file identities, text,
monotonically increasing revisions, and explicit import resolutions.

`LanguageService::snapshot()` retains an immutable project view. Queries return
owned diagnostics, definitions, references, hover text, and semantic tokens. Symbol
IDs never cross this interface. All editor ranges use zero-based UTF-16 positions;
the parser and analyzer continue using original UTF-8 byte ranges.

The published `@cross-css/native` package exposes `LanguageService` with `setFile`,
`removeFile`, `setResolution`, `document`, `definition`, `references`, and `hover`.
`languageServiceCapabilities()` supplies API version 1 and the ordered token legend.
`document()` returns a project revision, structured diagnostics, and absolute token
positions; adapters encode tokens for their protocol. Hover text is plain text.

The service reports syntax and language diagnostics. Backend-specific emission
errors still belong to compilation. Lint policy, completion, rename, and code fixes
are separate capabilities. Existing `Compiler` APIs remain available.


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
