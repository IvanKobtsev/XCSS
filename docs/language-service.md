# Language service

## Structured diagnostic messages (API v8)

Breaking: `document(uri).diagnostics` replaces `message` with ordered
`messageParts`. There is no compatibility message string. CrossBreeze and other
editor hosts must render the parts themselves, choosing their own code styling,
type notation, quoting, and escaping. XCSS still supplies the wording and part
order from the same message definition used by CLI and compiler text output.

| Part | Fields | Meaning |
| --- | --- | --- |
| `text` | `text` | Prose, spacing, and sentence punctuation. |
| `type` | `valueType` | Recursive `{ kind: 'unknown' }`, `{ kind: 'named', name }`, or `{ kind: 'union', members }`, in analyzer order. |
| `code` | `text` | Raw symbol spelling, directive, operator, path, or source snippet, without presentation quotes or markup. |
| `number` | `value` | An argument count. |

Only the fields applicable to a part are present. Type names, sigils, and source
spelling are preserved. Code, label, legacy code, severity, location, and related
locations retain their meanings. Labels are metadata: render `messageParts` as
the sentence rather than automatically prepending the label.

For example, XC-303 contains text `Argument of type `, the passed type, text
` is not assignable to parameter of type `, and the parameter type. XC-302 reads
`Expected 1-2 arguments, but got 0`, or `Expected 1 arguments, but got 0` when
the bounds match. Counts are number parts; the optional hyphen is text. The CLI
omits the label prefix for these two codes and preserves its other output layouts.

A minimal plain renderer can use the hover example's `formatType`:

```js
function formatDiagnostic(diagnostic) {
  return diagnostic.messageParts.map(part => {
    switch (part.kind) {
      case 'text': return part.text
      case 'code': return part.text // A rich UI can style and quote this separately.
      case 'type': return formatType(part.valueType)
      case 'number': return String(part.value)
    }
  }).join('')
}
```

Rust `ServiceSnapshot::diagnostics` returns `EditorDiagnostic` with `code`,
`severity`, `message_parts`, `label`, `legacy_code`, `location`, and `related`.
`DiagnosticPart` is an enum (`Text`, `Type`, `Code`, `Number`), and
`DiagnosticType` is recursive (`Unknown`, `Named`, `Union`). The previous nested
`diagnostic` field is removed. Text-based Rust hosts can use `plain_diagnostics`,
whose `PlainDiagnostic::diagnostic.message` is a `DiagnosticMessage` implementing
`Display`; call `to_string()` when an owned string is needed. Its CLI quoting
metadata never crosses the editor interface.

The internal `xcss_source::Diagnostic::message` is now `DiagnosticMessage`, not
`String`. Native compiler diagnostics and formatter results still return their
existing strings, rendered from that model. Separate editor warning APIs retain
their existing contracts. This change does not alter diagnostic locations,
validation rules, snapshot semantics, or CSS generation.

## Structured hover data (API v7)

Breaking change: `hover(uri, position)` returns semantic data instead of `text`.
Consumers of API v6, including CrossBreeze's Rust renderer, must replace access
to `hover.text` with their own formatting when adopting API v7. There is no
compatibility text field. API v8 also structures diagnostic messages, as above.

The result is `null` (Rust `None`) when no supported symbol resolves. Otherwise:

| Field | Meaning |
| --- | --- |
| `range` | Hovered occurrence's original zero-based UTF-16 range in the queried file; qualified references include the namespace prefix. |
| `kind` | `namespace`, `keyframe`, `variable`, `injection`, `function`, `util`, `parameter`, `constant`, or `class`. |
| `name` | Analyzer spelling, retaining sigils such as `$` and `--`. |
| `declaration` | Resolved declaration's location (`uri` and UTF-16 `range`), possibly in another file. |
| `valueType` | Structured analyzed type; for a function, its return type. |
| `signature` | For callables, ordered `parameters` and boolean `acceptsContent`; present even with zero parameters. |
| `variable` | For registered variables, boolean `inherits` and optional `initialValue`. |
| `constantValue` | For constants, the trimmed initializer source, without evaluation. |

A type is `{ kind: 'unknown' }`, `{ kind: 'named', name }`, or
`{ kind: 'union', members }`, with recursively structured members in analyzer
order. Unknown types are explicit, not missing. Each parameter has `name`,
`valueType`, and optional `defaultValue`. Defaults and initial values are XCSS
source strings without presentation labels. Inapplicable optional fields are
absent in Node results and `None` in Rust.

Rust exposes owned `Hover`, `HoverSymbolKind`, `HoverType`, `HoverSignature`,
`HoverParameter`, and `HoverVariable` types. Rust kinds and types are enums;
fields use snake_case and declaration locations use `file` instead of `uri`.
Internal analyzer symbol IDs do not cross the interface.

Hosts control all presentation. For example, a Node consumer can build a plain
signature from the data, then choose its own markup and escaping:

```js
function formatType(type) {
  switch (type.kind) {
    case 'unknown': return 'unknown'
    case 'named': return type.name
    case 'union': return type.members.map(formatType).join(' | ')
  }
}

function formatHover(hover) {
  if (!hover) return null
  if (hover.signature) {
    const parameters = hover.signature.parameters.map(parameter =>
      `${parameter.name}: ${formatType(parameter.valueType)}` +
      (parameter.defaultValue === undefined ? '' : ` = ${parameter.defaultValue}`))
    const content = hover.signature.acceptsContent ? ' { @content }' : ''
    const result = hover.kind === 'function' ? `: ${formatType(hover.valueType)}` : ''
    return `@${hover.kind} ${hover.name}(${parameters.join(', ')})${content}${result}`
  }
  return `${hover.name}: ${formatType(hover.valueType)}`
}
```

The hover target coverage and snapshot semantics are unchanged. This API does
not compile CSS, evaluate initializers, or add TypeScript hover targets.

## TypeScript editor integration (API v6)

The editor resolves named XCSS imports through the TypeScript project.
`definitionDetails(uri, position)` returns the original UTF-16 `origin`
range and all `targets`. `definition` continues to return just the targets.
Class accesses return every source selector that contributes to the export.
`cssVars` accesses resolve to their provided declaration, and the named
exports themselves navigate to the module or its corresponding directives.

`document(uri)` also returns TypeScript tokens for XCSS imports, class members,
and provided variable members. Their kinds are `xcssImport`,
`xcssClassMember`, and `xcssProvidedVariable`. Pass `false` as the optional
second argument when requesting diagnostics without tokens.

## Classes and variants (API v5)

`definition(uri, position)` and `references(uri, position, includeDeclaration)`
cover exported classes, variant groups, and variant members in stylesheets and
configured TypeScript projects. A class is keyed by stylesheet and name. A
variant member is also keyed by its group. A contextual TypeScript string
navigates to the member in its `@variants` list, and a member in the list
navigates to the class selector. Contextual values shared by multiple groups
can have multiple definitions.

`completion(uri, position)` returns `{ label, kind, range, newText }` entries
for supported stylesheet and TypeScript contexts. Apply each edit to its
original UTF-16 range. The kinds are `class`, `variantGroup`, and
`variantMember`. Dot access offers valid TypeScript identifiers, and bracket
access also offers names that need quoting.

Hosts with multiple TypeScript configurations can call `symbolsAt(uri, position)`
to get the export keys and pass them to `referencesForSymbols(keys, includeDeclaration)`
in each relevant project. Remap each key's `module` from the source snapshot's
file identity to the receiving snapshot's file identity. Each key contains
`module`, `kind`, `name`, and `group` for variant members.

## TypeScript projects (API v4)

Call `setTypescriptProject("tsconfig.json")` to enable XCSS-specific TS checking.
Supply TS/TSX sources, tsconfig files (including extended configs), package.json
files and dependency sources/declarations with `setTypescriptFile(path, revision,
source)`. Call `removeTypescriptFile(path, revision)` for deletion. Revisions must
increase, including across deletion and recreation. Pass `null` to
`setTypescriptProject` to disable checking. Config and dependency updates use the
same revisioned source API.

When enabled, use consistent project-relative forward-slash paths for both XCSS
and TS inputs. TS module resolution follows the supplied tsconfig, including
paths aliases. Host inputs must include all required dependencies; the engine
never reads the filesystem. Existing `setResolution` continues to resolve XCSS
injection imports.

`document(path)` and `xcss check` report XCSS diagnostics at their original source
locations. TypeScript diagnostics for other imports and type errors are omitted.

| Code | Error |
| --- | --- |
| `XC-001` | Unknown class on an XCSS styles export |
| `XC-002` | Invalid group argument to an XCSS `Variants` type |
| `XC-003` | Invalid value for an XCSS variant group |
| `XC-004` | Unresolved XCSS module import |

An unusable TypeScript project produces a standalone warning with the setup
error. Hosts can read it with `typescriptProjectWarning()`, which returns
`null` when TypeScript checking is disabled or setup succeeds. The CLI prints
it once and a warning alone does not fail the command. Run `xcss check` without
`--project` to check stylesheets alone.

Literal bracket access is checked even when the project disables
`noImplicitAny`. Normal TypeScript escape hatches remain available.
The service caches checking within an immutable project snapshot. Changes to
either language or project configuration invalidate it; retained snapshots keep
their previous diagnostics. TS checking currently rebuilds the checker after an
edit rather than reusing tsrs programs incrementally.

`xcss-language-service` provides editor queries without generation, filesystem
access, or a protocol dependency. Hosts provide normalized file identities, text,
monotonically increasing revisions, and explicit import resolutions.

`LanguageService::snapshot()` retains an immutable project view. Queries return
owned diagnostics, definitions, references, hover data, and semantic tokens. Symbol
IDs never cross this interface. All editor ranges use zero-based UTF-16 positions;
the parser and analyzer continue using original UTF-8 byte ranges.

The published `@cross-css/native` package exposes `LanguageService` with `setFile`,
`removeFile`, `setResolution`, `document`, `definition`, `references`, and `hover`.
`languageServiceCapabilities()` supplies the API version and the ordered token legend.
`document()` returns a project revision, structured diagnostics, and absolute
token positions. Adapters encode tokens for their protocol and format structured
hover data for their UI.

Diagnostics include a stable numeric `code`, a readable `label`, and the
ordered `messageParts`. Stylesheet codes use `XC-1xx` for syntax, `XC-2xx` for
binding, `XC-3xx` for types, and `XC-4xx` for variants. For example, `XC-105`
has the label “Invalid XCSS statement”. Hosts that previously matched
`syntax.invalid-statement` or another stylesheet identifier can use
`legacyCode` during migration. The complete mapping is in the
[diagnostic catalog](../crates/xcss-language-service/src/diagnostic_catalog.rs).

The service reports syntax and language diagnostics. Backend-specific emission
errors still belong to compilation. Lint policy, rename, and code fixes
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
Nested selectors use the same branch expansion as compiled CSS. An exact score
is returned only when every expansion of that source selector has the same tuple;
otherwise the score is unavailable with a reason explaining the varying branches.
Explicit `:is()`, `:not()`, and similar functions still use CSS specificity rules.
XCSS `:local` and `:global` wrappers are transparent. Scoped renaming does
not change specificity. At-rule boundaries add no specificity; an explicit
`:scope` counts as a pseudo-class. Keyframe steps are excluded.

Invalid/recovered selectors, unsupported functional selectors, unresolved parents,
and selectors in injection definitions whose call-site context is unknown have
no exact score. Other valid selector-list members remain queryable. This API does
not expand injection calls or provide lint thresholds or editor hover UI.

## Explicit class usage planning

`class_usage` (Rust) and `Compiler.planClassUsage(config, sources)` (native) replace
the fixed-slot planner. Sites identify source ranges, optional override argument
ranges, requested levels, and traced class origins; variant-prop sites translate
only the consumed xClass property. `clearClassUsage()` drops the active requests.
Generated artifacts expose `overrides[className][level]` runtime strings.

XC-010 reports invalid specificity literals, XC-011 untraceable or nullable
inputs, XC-012 repeated overrides, and XC-013 escaping helper invocations. These
checks share the unsaved project inputs used by editor and CLI diagnostics.
Authored selector specificity scores remain unchanged by helper usage.
