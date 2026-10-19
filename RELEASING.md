# Releasing XCSS

XCSS follows the same tag-driven release model as StyleContract. A `v*` tag starts `.github/workflows/release.yml`, which validates the repository, builds eight native targets, publishes their platform packages, and then publishes the native loader and Vite plugin.

## One-time npm setup

All packages are public under the `@cross-css` organization:

- `@cross-css/vite-plugin`
- `@cross-css/native`
- `@cross-css/native-darwin-arm64`
- `@cross-css/native-darwin-x64`
- `@cross-css/native-linux-arm64-gnu`
- `@cross-css/native-linux-arm64-musl`
- `@cross-css/native-linux-x64-gnu`
- `@cross-css/native-linux-x64-musl`
- `@cross-css/native-win32-arm64-msvc`
- `@cross-css/native-win32-x64-msvc`

Do not run `npm publish` from `packages/native-platforms`. Those directories are private dependency-resolution stubs and do not contain native binaries.

For the first release, create a granular npm access token that can publish public packages under the `@cross-css` organization and add it to the GitHub repository as the `NPM_TOKEN` Actions secret. Push the first matching `v*` tag; the workflow will build and publish the real platform tarballs with public access.

After the first release, configure each package's npm trusted publisher with:

- Provider: GitHub Actions
- Organization or user: `IvanKobtsev`
- Repository: `XCSS`
- Workflow filename: `release.yml`
- Allowed action: `npm publish`

Complete the first trusted publish within npm's validation window, then delete the `NPM_TOKEN` repository secret. Subsequent releases use npm OIDC and provenance without a long-lived publishing token.

## Create a release

1. Update the version in the root `package.json`, both publishable package manifests, and `[workspace.package]` in `Cargo.toml`.
2. Update the exact `@cross-css/native` dependency in the Vite plugin, all eight native optional dependency versions, and the versions in the eight private workspace stubs.
3. Run `cargo check` so `Cargo.lock` receives the new workspace version.
4. Run `corepack pnpm install`, `corepack pnpm verify-release`, and the full test suite.
5. Commit and push the version change.
6. Create and push a matching tag, for example `v0.2.0`.

The workflow rejects a tag whose version does not exactly match the manifests. Publishing is restart-safe: package versions already present on npm are skipped when a partially completed workflow is rerun.
