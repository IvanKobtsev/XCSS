# Publishing Rust crates

The Rust crates share the workspace version. Internal dependencies specify both a
local path and an exact registry version; update both when releasing a new version.
The Node binding is npm-only and must remain `publish = false`.

## Validation

Use the toolchain in `rust-toolchain.toml`:

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

Run `cargo package --list -p NAME` for each crate and inspect the included files.
Before publishing each crate, run `cargo publish --dry-run --locked -p NAME`.
This packages and compiles the registry form of the crate. Dependent crates cannot
complete their first dry run until their dependencies exist on crates.io.

## First publication order

1. xcss-source
2. xcss-syntax
3. xcss-analysis
4. xcss-typescript
5. xcss-language-service
6. xcss-format
7. xcss-codegen
8. xcss-core
9. xcss-resolver
10. xcss-cli

Authenticate with crates.io using `cargo login` or a scoped CI credential. Check
crate-name availability and ownership before the first release. Run the dry run,
review its output, then publish that crate with `cargo publish --locked -p NAME`.
Wait for its registry version to become available before processing dependents.
Do not use `--no-verify` to bypass package compilation failures.

The existing npm release workflow does not publish Rust crates. The publish-crates.yml workflow publishes Rust crates on v* tags or manual dispatch,
using the crates-io GitHub environment and crates.io trusted publishing.
Keep third-party notices, including the Apache-2.0 tsrs notice, in distributions.
After publishing, verify a standalone consumer of xcss-language-service,
xcss-format, and xcss-source using registry dependencies without path patches.

## PowerShell helper

After committing the release changes and running `cargo login`, use:

```powershell
./scripts/publish-crates.ps1 -WhatIf  # Preview the publication order without uploading
./scripts/publish-crates.ps1          # Validate and publish in dependency order
```

The script reads versions from Cargo metadata, skips existing non-yanked versions,
stops on errors, and waits for each uploaded version to appear on crates.io.
Rerun it to resume an interrupted release. Cargo also waits for index availability
before returning from publication. Credentials are read by Cargo, never by the script.
Use `-Confirm` to review each publication or `-DryRun` to validate only. On a first
release, dependent dry runs require their dependencies to have been published first.
The script does not bypass Cargo's clean-worktree or package verification checks.

## Bumping the release version

```powershell
./scripts/bump-version.ps1 0.13.0 -WhatIf
./scripts/bump-version.ps1 0.13.0
```

This synchronizes the root npm package, all packages under packages/, the Rust
workspace, exact internal dependency versions, and both lockfiles. Versionless
private fixtures retain their workspace dependencies. Cargo, Node, and Corepack
are required. Cargo uses cached registry metadata; pnpm may need network access.
If lockfile generation or verification fails, original file contents are restored.
Review and commit the result, then create the matching v0.13.0 release tag.
