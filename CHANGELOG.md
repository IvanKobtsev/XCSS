# Changelog

## 0.7.0

- Add module-local `@variants` groups with validation and formatting support.
- Embed tsrs to check exact XCSS class exports and variant values across TypeScript and JSX files.
- Bundle permissive TypeScript declarations at `@cross-css/vite-plugin/client`.
- Add `xcss check --project tsconfig.json` and native language-service API v4 for revisioned TS project inputs.
- Share static class-export contracts between analysis and CSS generation, including injection content.
- Support silent line comments and infer unannotated literal zero as a length while retaining explicit numeric annotations.
- Require Rust 1.99 when building from source. Published native packages include the checker and standard libraries.

Variant checking does not add runtime `xClass` support or convert variant strings to scoped class names.
