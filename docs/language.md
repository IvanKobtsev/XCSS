# XCSS language engine

XCSS extends modern CSS and CSS Modules, not Sass. Native nesting, unknown CSS
properties and extensible at-rules are accepted. Native CSS imports are distinct
from `@import namespace from "file"` and `@import { name } from "file"`.

Extensions include top-level `@vars { [local] --name[: type] [= initial]; }`,
`@injection name($parameter: type [= default]) { ... }`, `@:name(arguments)`
with an optional content block, and `@content;` inside injections. Parameters
are lexical. Names are case sensitive; hyphens and underscores are distinct.
Local injection definitions take precedence over named imports, then globally
unique definitions. Namespace calls use `@:namespace.member(...)`.

Values support bare custom-property references, arithmetic and transition
property groups. `content: preserve` is a generator directive. Undeclared custom
properties remain global. Dynamic values have unknown types, not type errors.
Mutable Sass variables, interpolation, module directives, mixins and control
flow are rejected. CSS Modules classes are local unless marked `:global`.

## Compile-time constants

Declare constants with `$name = value;` or `$name: type = value;`. An initializer
is required. Names follow the same case-sensitive, escape-normalized identity as
injection parameters; hyphens and underscores are distinct. Constants use the
existing XCSS type names and compatibility rules. Inference follows references
to constants, parameters, and typed custom properties, including dimensional
arithmetic such as length divided by number. An unknown inferred type requires
an explicit annotation; annotated dynamic CSS expressions remain permitted.

```xcss
@vars { --my-height: length = 4px; }
$base = 56px;
$height = $base + --my-height / 2;
$gradient = linear-gradient(to right, #000 --start, #fff (--end + --offset));

.header {
  $padding: length = 8px;
  height: $height;
  padding: $padding;
  background: $gradient;
  &:hover { padding: $padding * 2; }
}
```

Constants may be declared at stylesheet level or inside statement blocks. Each
selector, at-rule block, injection body, and supplied content block has lexical
scope. A declaration becomes visible after its initializer and remains visible
to descendant blocks. Siblings and separate blocks with the same selector do not
share local declarations. Top-level constants are file-local. Referencing an
undeclared, later, or self-referencing constant is an error. Redeclaration and
shadowing of visible constants or parameters are always errors, even if the new
value is identical. There is no reassignment or constant import/export syntax.

References work in CSS declaration values, other constant initializers, injection
arguments/defaults, and `@media`, `@supports`, and `@container` conditions. They
do not interpolate selectors, property names, import paths, URL contents, or
declaration names. Injection bodies capture definition-site constants; supplied
content uses caller scope. Injection-local constants may depend on parameters
and resolve independently for each expansion.

Compilation substitutes values and removes constant declarations. It does not
create CSS custom properties or registrations for constants. Bare custom-property
references and arithmetic retain normal XCSS lowering, for example
`$height` above emits `calc(56px + var(--my-height) / 2)` (with the registered
property's scoped name). Arithmetic substitutions preserve grouping. CSS runtime
expressions, relative units, and custom properties may still change the computed
value in the browser; the constant binding itself cannot change.

Parsing always retains original text and byte locations, including erroneous
regions. Missing delimiters produce diagnostics and recovery markers. Analysis
never generates code or reads files. Future emission targets pure CSS Modules.

Generation emits scoped CSS and class-export dictionaries. Typed variables without
initial values use documented defaults; untyped variables without values use
wildcard registration. See [generation](generation.md).
