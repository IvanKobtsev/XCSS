# Releasing XCSS

A `v*` tag starts `.github/workflows/release.yml`. It validates the repository, builds eight native targets with the CLI, and publishes the platform packages and native loader. It also publishes the Vite package after its initial manual release.

## First release of `@cross-css/vite`

1. Push the release commit and the `v0.7.2` tag. Wait for the **Release** workflow to succeed and publish the nine native packages at `0.7.2`.
2. Download and extract the **cross-css-vite-v0.7.2** artifact from that run. Sign in to npm with an account authorized to publish under `@cross-css`, then publish the tarball.

   ```sh
   npm publish ./cross-css-vite-0.7.2.tgz --access public
   npm view @cross-css/vite@0.7.2 version bin
   ```

3. Configure trusted publishing for `@cross-css/vite` using the settings below. Test a fresh install of `@cross-css/vite@0.7.2` by running the project's `check:xcss` script.

## One-time npm setup

All packages are public under the `@cross-css` organization:

- `@cross-css/vite`
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

After a package's first publish, configure its npm trusted publisher with the following settings.

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
