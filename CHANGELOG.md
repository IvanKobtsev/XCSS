# Changelog

## 0.10.0

- Add `@provided` custom properties, the named `cssVars` export, and React's `xVars` prop for inline style overrides.
- Add order-based specificity to React's `xClass` prop. XCSS checks fixed slots and resolves scoped classes and variant groups across components.
- Replace the default class map export with the named `styles` export. Keep default imports for `?inline` and `?url`.
- Reject `composes` declarations in XCSS. Use `@util` to share styles or `@raw` to pass CSS through.

## 0.9.0

- Add native editor API v5 for class, variant group, and variant member navigation, references, and completion in XCSS and TypeScript.

## 0.7.2

- Rename the Vite package to `@cross-css/vite`. Update the Vite config import and any declaration references to `@cross-css/vite/client`.
- Include the `xcss` command with the Vite package. The platform dependency supplies the precompiled checker, so CI can run `xcss check --project tsconfig.app.json` from a package script after installing dependencies.
- Load declarations from dependency build directories and avoid a tsrs file-loading crash on large projects.

## 0.7.1

Version 0.7.0 was not published because native target setup failed. This release
also aligns the cross-platform build action with the pinned Rust toolchain.

- Add module-local `@variants` groups with validation and formatting support.
- Embed tsrs to check exact XCSS class exports and variant values across TypeScript and JSX files.
- Bundle permissive TypeScript declarations at `@cross-css/vite-plugin/client`.
- Add `xcss check --project tsconfig.json` and native language-service API v4 for revisioned TS project inputs.
- Share static class-export contracts between analysis and CSS generation, including injection content.
- Support silent line comments and infer unannotated literal zero as a length while retaining explicit numeric annotations.
- Require Rust 1.99 when building from source. Published native packages include the checker and standard libraries.

Variant checking does not add runtime `xClass` support or convert variant strings to scoped class names.
