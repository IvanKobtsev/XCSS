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
`languageServiceCapabilities()` supplies API version 3 and the ordered token legend.
`document()` returns a project revision, structured diagnostics, and absolute token
positions; adapters encode tokens for their protocol. Hover text is plain text.

The service reports syntax and language diagnostics. Backend-specific emission
errors still belong to compilation. Lint policy, completion, rename, and code fixes
are separate capabilities. Existing `Compiler` APIs remain available.


## CSS declaration validation inputs (API v2, native 0.3.2)

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
descriptor at-rule bodies are excluded. Conditional blocks nested in rules inherit their declaration context. Rules nested inside at-rules remain
eligible. Consumers may substitute representative CSS values for typed parts;
this is a broad compatibility check, not proof for all values of that type.
Changes to any file or resolution invalidate the snapshot revision.

## Selector analysis and specificity (API v3)

`LanguageService.selectors(uri)` returns `{ revision, selectors }` without
compiling CSS. Each selector-list member has its original UTF-16 `range`,
non-overlapping `parts` (`kind`, `range`), and either `specificity`
(`ids`, `classes`, `types`) or an explanatory `reason` when an exact
score is unavailable. Compare specificity tuples lexicographically, never as
decimal numbers. This is selector specificity, not full cascade priority.

Classification covers classes, IDs, tags, attribute names, pseudo-classes,
pseudo-elements, universal selectors, nesting, combinators, and namespace prefixes.
Attribute strings and numbers keep lexical highlighting. Functional pseudo names
include colon prefixes, while their selector arguments have independent parts.
The existing eight semantic token indices are stable; API v3 appends
`selectorId`, `selectorTag`, `selectorAttribute`, `pseudoClass`,
`pseudoElement`, `selectorUniversal`, `selectorNesting`,
`selectorCombinator`, and `selectorNamespace`. IDs no longer use `variable`.

Specificity follows [MDN](https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Cascade/Specificity/)
and the linked CSS specifications: zero-weight `:where()`, maximum arguments
for `:is()`, `:not()`, and `:has()`, nth-child selector filters,
shadow DOM selectors, legacy pseudo-elements, and implicit/explicit nesting.
Parent lists contribute their maximum tuple; repeated `&` contributes repeatedly.
XCSS `:local` and `:global` wrappers are transparent. Scoped renaming does
not change specificity. At-rule boundaries add no specificity; an explicit
`:scope` counts as a pseudo-class. Keyframe steps are excluded.

Invalid/recovered selectors, unsupported functional selectors, unresolved parents,
and selectors in injection definitions whose call-site context is unknown have
no exact score. Other valid selector-list members remain queryable. This API does
not expand injection calls or provide lint thresholds or editor hover UI.
