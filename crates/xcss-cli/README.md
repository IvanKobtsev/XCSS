# xcss-cli

Part of [XCSS](https://github.com/IvanKobtsev/XCSS), a typed styling language.

See the repository documentation for language syntax and API usage.

## CrossFigma import

Run `xcss cross-figma import design.crossfigma.json` to import a CrossFigma v1
snapshot, or `xcss cross-figma receive` to wait for one successful transfer from
the Figma plugin. Both accept `--root <directory>` (default: current directory).
The receiver uses loopback port 38473 and exits after writing the files; Ctrl+C
stops it. Invalid transfers return diagnostics and leave it waiting.

Create `xcss.config.json` in the project root:

```json
{
  "cross-figma": {
    "output": "./src/figma",
    "screens": {
      "tablet": {
        "media": "(width >= 768px) and (width < 1024px)",
        "modes": { "Spacing": "Tablet" }
      }
    },
    "themes": {
      "light": { "modes": { "Colors": "Light" } },
      "black": { "modes": { "Colors": "Dark" } }
    },
    "units": {
      "exact": { "opacity": "" },
      "prefix": { "spacing/": "px" }
    }
  }
}
```

Collection and mode names match exactly. Collection defaults populate
`globals.gen.xcss`; mapped modes generate changed custom properties in
`tablet-media.gen.xcss` and `[data-theme="..."]` selectors in `themes.xcss`.
Themes and screens are independent. Unmapped modes produce warnings.
`typography.gen.xcss` contains a utility per text style.

Output paths are relative to the project root. Files are overwritten, but obsolete
files are not deleted. Import the generated files from your application stylesheet
in the desired cascade order; imports are not inserted automatically.

Variable and utility names become lowercase kebab-case; numeric-leading names
receive `n-`. Collisions fail the import. Aliases become `var(--name)` and must
resolve locally without cycles. Colors become RGB with alpha, strings are quoted,
booleans remain true/false. Numbers are unitless unless a unit rule matches their
original name: exact rules win, then the longest prefix. Units are appended without
conversion (a `rem` rule does not divide pixel values).

Typography includes family, inferred weight/style from the font style name,
size, line height, letter spacing, text case, decoration, and paragraph indent.
Typography lengths use pixels; percentage line height becomes a ratio and
percentage letter spacing becomes em. Font size and paragraph indent variable
bindings are retained. Other bindings, unsupported Figma-only typography
properties, and paint/effect/grid styles produce warnings.

## CrossFigma update

Run `xcss cross-figma update design.crossfigma.json`, or run
`xcss cross-figma update` to receive the snapshot from Figma. Both accept
`--root <directory>`. Imports and updates save `.cross-figma.json` in the
output directory; keep this ID-to-name manifest with the generated files.
For existing projects, import the previous snapshot once before updating.

Update regenerates outputs and renames imported custom properties throughout
project `.xcss` files in two in-memory passes (old names to ID tokens, then
ID tokens to new names). Swaps and rename cycles preserve variable identity.
Comments and strings are preserved. Dependency, build, and version-control
directories and symlinks are skipped. Deleted IDs produce warnings and their
references remain unchanged. Conflicting destination declarations fail before
writing. Source changes, generated files, and metadata are committed together
with rollback on installation failure. Obsolete generated files are not deleted.
