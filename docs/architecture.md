# Language engine architecture

The compiler pipeline is:

```text
xcss-source <- xcss-syntax <- xcss-analysis <- xcss-codegen
                       xcss-core (public facade)
                       /        |        \
                     CLI       Node      Vite
```

Source owns immutable revisions, file identity, original byte ranges, line indexes,
UTF-16 conversion, and diagnostics. Syntax owns trivia-preserving tokens,
recovering trees, typed AST views, arithmetic precedence, and selector inspection.
Raffia stays private to syntax. Analysis owns scopes, symbols, bindings, types,
injection targets/bodies, constant initializers, registrations, keyframes, and dependencies.
Code generation consumes those facts, expands injections into an origin-aware IR,
lowers extensions, then uses pinned Lightning CSS privately for scoping/printing.
The independent `xcss-language-service` consumes source, syntax, analysis, and
the isolated `xcss-typescript` integration.
It returns owned editor facts and UTF-16 ranges without invoking generation.

Analysis exposes module interfaces (exported local class names and variant
groups), shared with generation. `xcss-typescript` embeds a pinned tsrs revision
behind a host-supplied in-memory filesystem. Per-module synthetic declarations
replace permissive XCSS types only inside that checker. Comparing precise and
permissive checks filters unrelated TS errors. Project configuration and required
resolution failures remain visible. No synthetic declarations are written to disk.
The workspace pins Rust 1.99 for this integration.

The facade contains no language implementation. Filesystem discovery, alias
resolution, output writes, CSS delivery, and asset loading belong to adapters.
The old Sass-backed compiler and shared-partial pipeline have been removed.

`node npm/check-architecture.mjs` enforces dependency allowlists and rejects
standard-library host I/O in the engine and facade. It includes negative controls
and runs on Windows and Linux CI. Public types expose neither parser nor backend
objects.

## Analysis host and snapshots

`AnalysisHost` accepts source additions/replacements/removals with increasing
revisions, plus explicit import-resolution updates. Deletion revisions are retained.
Project changes reuse unchanged parse trees and invalidate semantic analysis.
The next snapshot rebuilds semantic facts, including the global injection index.
Immutable snapshots remain valid after subsequent edits. Symbol/scope IDs belong
to one snapshot. Positions use zero-based UTF-16 units; byte ranges use UTF-8.

Queries expose syntax, scopes, symbols, references/targets, signatures, expression
structures/types, registration facts, diagnostics and dependencies.
An unresolved custom property remains global. Dynamic types remain unknown.
Missing imports suppress consequential unresolved-call diagnostics. CSS runtime
values are never evaluated by the analyzer.

## Generation and adapters

`generate(snapshot, entry, options)` returns scoped module artifacts and warnings,
or original-source diagnostics without artifacts. Only errors in required files
block an entry. Injection modules only
contribute definitions. Optional bundling supports inline/URL requests.

Artifacts carry CSS, class dictionaries, custom-property names, source maps, native
imports, stylesheet dependencies, and source dependencies. Module IDs supplied to
generation must be project-relative. CSS names depend on module identity, not
absolute machine paths or source content. Definition-owned variables/URLs and
caller-owned arguments/content retain their origins during expansion.

The CLI stages the full output set before replacement. Node exposes a reusable
Compiler with declarations generated from binding metadata. Vite uses JavaScript
wrappers and adjacent plain virtual CSS. HMR invalidates active XCSS consumers;
project synchronization commits files and resolutions together after asynchronous
loading so stale reads cannot overwrite a newer project version.

See [generation](generation.md) for lowering and registration rules. Future work
adds further compile-time language features, lint policy/fixes, completion/rename, migration
adapters.
