# XCSS language engine

## Variant groups

`@variants Button: .primary, .secondary;` declares a top-level, module-local,
case-sensitive group. The name is independent of selectors. Members must be
unique exported local classes; forward references are allowed. Empty lists,
duplicate groups, missing classes and nested declarations are errors.
Classes introduced by called injections belong to the calling module. Unused
injection definitions and `:global` classes are not module exports.

The directive emits no CSS. TS consumers import `type Variants` from the same
XCSS module and write `Variants<"Button">`. XCSS analysis checks it as the exact
union `"primary" | "secondary"`, including assignments and JSX props across
files. Other exported classes remain accessible through the default styles map
without becoming variants. Arbitrary strings fail union assignment; ordinary TS
assertions and `any` retain their usual behavior.

The project's own TS checker sees a permissive string type through
`@cross-css/vite/client`. Exact checking runs in the native language service
or `xcss check --project tsconfig.json`. This does not add runtime conversion of
variant strings or an `xClass` transform.

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

## Zero inference and comments

An unannotated literal `0` defaults to `length`, so `@vars { --gap = 0; }`
registers a length variable. Explicit `number` and `integer` annotations and
parameters still accept literal zero. Other spellings, such as `0.0`, `-0`,
`+0`, and `0e0`, retain their numeric inference. Zero used as an arithmetic
operand retains its numeric role.

XCSS supports block comments (`/* ... */`) and single-line comments (`// ...`).
A line comment ends at the next LF, CRLF, CR, or end of file. Comments do not
replace required semicolons. The formatter preserves line comments; generated
CSS omits them. Both `// prettier-ignore` and `/* prettier-ignore */` preserve
the next statement's formatting.

```xcss
// Default to a length without writing 0px
@vars { --gap = 0; }
.card {
  margin: --gap; // This comment is omitted from generated CSS
}
```

Comment markers inside quoted strings and unquoted `url(...)` values remain
literal, including `url(https://example.com/image.png)` and
`url(//cdn.example.com/image.png)`. Raw CSS blocks retain their existing
verbatim behavior; `//` is not interpreted as an XCSS comment inside them.

## Raw CSS

`@raw { ... }` is an escape hatch for CSS that XCSS cannot handle. It is allowed
wherever statements are allowed, including inside rules, injections and utilities.
The wrapper is removed and everything between its braces is emitted byte for byte,
including whitespace and comments, even with minification enabled.

```xcss
@raw {
  .unscoped { color: var(--global-color); }
}
```

Raw contents receive no CSS or XCSS validation, scoping, substitutions, URL rebasing,
dependency discovery, exports or editor intelligence. Names and URLs must already
be correct for the output. Injections copy raw bodies literally; parameter names
inside them are not substituted. A nested `@raw` inside a raw body remains literal.
Only balanced braces, strings, comments and escapes are scanned to find the end.
The `@raw` header accepts no arguments and requires a block.

## Utilities

`@util` declares a reusable, typed style recipe:

```xcss
@util truncate-multiline($lines: integer = 3) {
  display: -webkit-box;
  -webkit-line-clamp: $lines;
  -webkit-box-orient: vertical;
  overflow: hidden;
  max-height: $lines * 1lh;
}

.someText {
  @:truncate-multiline(2);
  color: white;
}

span[data-truncate] {
  @:truncate-multiline;
  color: black;
}
```

For a single local class, or a list consisting entirely of single local classes
(`.a, :local(.b)`), the exported class strings include a generated utility class,
such as `_util_truncate-multiline_2`. Its styles are deduplicated in a shared
stylesheet, inside the top-level `@layer utils`. Compound classes, descendants,
pseudo-classes, and mixed/global selector lists instead receive separate rules in
their own stylesheet. These pair the caller selector with a readable, escaped
DevTools marker such as `_util_inj_\[truncate-multiline_3\]`.

`@:truncate-multiline(3) as injection;` forces that selector-based output for any
selector. It still uses the top-level `utils` layer and does not insert declarations
into the caller's ordinary rule. The marker element names are reserved for XCSS.
Declare an explicit layer order, for example `@layer utils, components;`, when
utility precedence matters; normal unlayered declarations outrank layered styles.

Util calls may form a prefix group, a suffix group, or both. A call between two
ordinary properties is an error. Comments and compile-time constants do not count
as properties. Calls require a selector, including when expanded through an
injection; calling a util directly at stylesheet root is invalid.

Utils share injection parameters, defaults, lexical constants, imports, namespaces,
type checking, and lookup precedence. Equally visible callable names are ambiguous;
use imports and namespaces to disambiguate definitions in different files. Utils
can nest rules and call other utils or injections; inner calls expand into the
outer recipe. Local class selectors, `@content`, and supplied content blocks are
forbidden, including when introduced through another call. Class selectors inside
a util must be explicitly `:global`. Ordinary injections retain their existing
expansion and content behavior.

Resolved default and explicit arguments share a utility rule. Insignificant
formatting is normalized, but mathematically equivalent expressions are not
evaluated for deduplication. Different runtime conditions retain separate rules.
Complex argument labels and conflicting definition names receive stable suffixes.
Bare words remain CSS literals: use `$lines`, not `lines`, to substitute a parameter.

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
