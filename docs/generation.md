# CSS generation

## Contract

`xcss-codegen::generate(snapshot, entry, options)` emits readable scoped CSS by
default, class dictionaries, declared custom-property names, source maps, and
native/composition/source dependencies. `GenerateOptions` supports minification,
per-module output identities for URL rebasing, and composition-only bundling.
`xcss-core` re-exports these APIs and provides project inspection/compilation.

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
