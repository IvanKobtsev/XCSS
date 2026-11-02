# CSS generation

## Contract

`xcss-codegen::generate(snapshot, entry, options)` emits readable scoped CSS by
default, class dictionaries, declared custom-property names, source maps, and
native/composition/source dependencies. `GenerateOptions` supports minification,
per-module output identities for URL rebasing, and composition-only bundling.
`xcss-core` re-exports these APIs and provides project inspection/compilation.

`GeneratedProject.utilities` optionally contains shared composed-utility CSS,
its source map and source dependencies, and independently mapped rules with an
explicit first-use order. `utilities_output_path` sets its destination for asset
rebasing. `merge_utilities` unions artifacts from multiple entries by definition,
resolved arguments, and enclosing runtime conditions. Generated utility classes
are appended to class-export values; they are not additional export keys.

All utility styles use the top-level `utils` layer. Composed rules live in the
shared artifact; selector-based calls stay in their caller's artifact. Nesting
selectors resolve before extraction, and media/supports/container conditions are
preserved. Explicit top-level layer-order statements are copied into the shared
prelude so importing utilities does not establish an unintended layer order.
Consequently a shared artifact may contain only layer-order statements.

The CLI writes `<output>.utils.css` (for example `out.utils.css`), with an adjacent
source map, and imports it from generated module stylesheets. Bundling includes
the shared rules once and returns no separate utility artifact. Native Node
compilation exposes `utilities`; `Compiler.compileUtilities(entries)` collects a
deduplicated union for hosts delivering several independent entry modules.

Vite serves a single shared virtual CSS module during development and refreshes
it when calls change or files disappear. Production emits one utility CSS asset
after discovering the complete graph, including lazy modules. HTML entries link
that asset; a shared JavaScript loader also handles non-HTML and lazy entry usage.
Inline and URL stylesheet requests include their own bundled utilities.

Class, ID and animation scoping use pinned Lightning CSS with
`xcss_[hash]_[local]` and project-relative filenames. Dashed-identifier scoping is
disabled: only declared XCSS custom properties receive an XCSS module suffix.
Composition exports are resolved transitively and deduplicated in source order.
Only local classes are included in the default JavaScript dictionary. Global
keyframe exceptions are normalized through private backend placeholders.

Lowering retains source spans for declarations, values, substituted arguments,
and content. Backend rule maps are composed with lowering maps and enriched with
declaration/value-token origins. Serialization may normalize equivalent values
(e.g. calc(10px + 2px) to 12px); such values map to their source expression.

## Lowering

Compile-time constant declarations are removed. References substitute initializer
values using definition-site bindings and source spans; parameter-dependent
constants resolve separately for each injection expansion. Substituted arithmetic
retains grouping, URLs retain definition-file origins, and runtime CSS expressions
remain dynamic. Constant expansion is limited to 128 nested substitutions and
100,000 substitutions per entry.

Bare custom-property values become var() calls; names remain names in declarations,
registrations and transition property positions. Arithmetic emits calc() while
respecting CSS slash separators and existing math functions. Parentheses resolve
mixed-value ambiguity. Transition groups expand shared timing values, with
allow-discrete for display/content-visibility/overlay unless normal is explicit.

Injections expand in statement order. Classes introduced by expansion belong to
the caller; definition-owned variables and URL origins remain in their defining
module. Arguments and content retain caller bindings and source spans. Content
can be substituted multiple times. Recursive cycles fail analysis; expansion has
limits of 128 nested calls and 100,000 visited expansion statements per entry.
Eligible pseudo-element rules receive empty content after expansion. Existing
content wins; content: preserve suppresses insertion and is removed.

Native CSS nesting is preserved. License comments survive; exact formatting and
ordinary comments are not guaranteed. Browser targets/prefixing are downstream
responsibilities. Unknown declarations/at-rules are retained when supported by
the backend; backend failures produce diagnostics rather than silent omission.

`@raw` bodies bypass lowering and the CSS backend. Their wrappers are removed,
and their exact bytes survive minification, injection/utility expansion and
bundling. Raw imports stay in place and are not dependencies. Source maps retain
raw definition locations and positions after each inserted body. An internal
`x_xcss_raw` source-map extension preserves opaque boundaries when artifacts are
subsequently merged; hosts should retain it when passing artifacts back to XCSS.

## Variable registration

- Initial value only: infer type, or report inference failure.
- Type and initial value: validate both CSS type and computational independence.
- Type only: use the default below.
- Neither: syntax "*", no initial value.
- Local means inherits:false; other registrations inherit.

| Type | Default |
| --- | --- |
| number, integer | 0 |
| percentage | 0% |
| length, length-percentage | 0px |
| angle | 0deg |
| time | 0s |
| resolution | 1dppx |
| color | transparent |
| image | linear-gradient(transparent, transparent) |
| url | url("data:,") |
| transform-function, transform-list | matrix(1, 0, 0, 1, 0, 0) |
| frequency | 0Hz |
| flex | 0fr |

Frequency/flex use browser wildcard syntax with warnings and retain static XCSS
types. Other typed registrations reject dependent values such as var(), em/rem,
container-relative units, and currentColor. Registrations required by imported
injections are included without importing unrelated library styles.

## Integration limits

Composition supports project XCSS modules and global names; external CSS Modules
need a separate adapter. Value imports/exports and ICSS syntax are not added.
No Sass variables, loops or functions are supported. Compilation errors do not
replace CLI output. CLI names are plain .css, with adjacent .css.map and
.exports.json artifacts. Native CSS imports and assets remain downstream inputs;
the generator rebases them according to their origin and output location.
