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
General Sass variables, interpolation, module directives, mixins and control
flow are rejected. CSS Modules classes are local unless marked `:global`.

Parsing always retains original text and byte locations, including erroneous
regions. Missing delimiters produce diagnostics and recovery markers. Analysis
never generates code or reads files. Future emission targets pure CSS Modules.

Generation emits scoped CSS and class-export dictionaries. Typed variables without
initial values use documented defaults; untyped variables without values use
wildcard registration. See [generation](generation.md).
