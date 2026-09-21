# Contributing

Thanks for working on `audiowaveform`.

The project is a Rust workspace centered on the `audiowaveform` library crate,
with `audiowaveform-cli` as a thin adapter. Reusable logic belongs in the
library. CLI-only argument parsing, user-facing logging, and exit handling
belong in the binary crate.

## Before You Start

- Discuss substantial feature work before implementing it.
- Work from an up-to-date branch.
- Keep changes focused; avoid mixing refactors, behavior changes, and fixture updates unless they are directly related.

## Development Expectations

- Follow the existing Rust style and module boundaries.
- Keep the public API narrow and ergonomic.
- Every public struct, enum, function, and public method must remain documented.
- Update rustdoc examples and README examples when public behavior changes.
- Add or update tests for behavior changes. Prefer unit tests for internal logic and integration tests for end-to-end workflows.

## Local Checks

Run these before sending a change for review:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

If you touch feature-gated code in the library, also run the feature matrix used in CI:

```sh
cargo test -p audiowaveform --no-default-features
for feature in decode format-aac format-aiff format-caf format-flac format-m4a format-mp4 format-mkv format-webm format-mp1 format-mp2 format-mp3 format-ogg format-wav render wav-output; do
  cargo test -p audiowaveform --no-default-features --features "$feature"
done
cargo test -p audiowaveform --all-features
cargo test -p audiowaveform-cli --no-default-features
cargo test -p audiowaveform-cli --no-default-features --features format-mp3,format-m4a
cargo test -p audiowaveform-cli --no-default-features --features render
cargo test -p audiowaveform-cli --no-default-features --features wav-output
```

When changing the Ruby binding, also run:

```sh
bundle install
bundle exec rake
bundle exec rake build
```

## Fixtures and Goldens

- Shared fixtures and goldens live in `fixtures/`.
- `fixtures/formats/generate.py` regenerates the synthetic format fixtures with
  Python 3 and FFmpeg. Tests use the committed files and do not invoke FFmpeg.
- If you intentionally change rendered output or decoder behavior, update the relevant golden files and explain why in the change.

## Releases

Release preparation should be a dedicated change. For a Rust library/CLI release:

- Update `[workspace.package].version` in `Cargo.toml` and the library dependency
  version in `crates/audiowaveform-cli/Cargo.toml` together.
- Update the install/dependency examples in `README.md` and `CHANGELOG.md`.
- Refresh both `Cargo.lock` and `bindings/ruby/Cargo.lock` so they contain the
  new library version. Keep the Ruby gem's version unchanged.
- Run the local checks above and `cargo package --workspace --all-features --locked`.
  Workspace packaging/publishing requires Cargo 1.90 or newer and network access
  to resolve the CLI's dependency on the library before its first publication.
- Merge the preparation PR, then create and push an annotated `rust-vX.Y.Z` tag
  on that merged commit. Historical unprefixed tags belong to the C++ releases.

Rust `0.1.0` is the first crates.io release. The API is still evolving; breaking
changes may occur in subsequent `0.x` minor releases.

The `Rust Crate Release` workflow checks the tag against the workspace version,
runs the full Rust CI suite and verifies both packaged crates, then publishes
the library followed by the CLI to crates.io. It also attaches their `.crate`
archives to a GitHub release. Manual workflow runs validate and package only;
they never publish. Integration tests use fixtures outside the crate directories,
so they run from the checkout in CI and are omitted from published archives.

### crates.io setup

Before pushing the first Rust tag:

1. Create a GitHub environment named `rust-release`, allowing deployment only
   from tags matching `rust-v*`. It is separate from Ruby's `release` environment.
2. Create a crates.io API token authorized to publish `audiowaveform` and
   `audiowaveform-cli`, including their first publication. Add it as the
   `CARGO_REGISTRY_TOKEN` secret in the `rust-release` environment. Do not commit
   or paste the token into an issue or pull request.

The initial publication requires an API token. Once both crates exist,
[crates.io Trusted Publishing](https://crates.io/docs/trusted-publishing) can be
configured for this repository, the `rust-release.yml` workflow and the
`rust-release` environment; switch the workflow to OIDC authentication before
removing the API-token secret.

### Recovering an interrupted Rust release

Crate versions are immutable. If publishing stops after the library succeeds,
check out the same release tag and publish only the missing CLI with
`cargo publish -p audiowaveform-cli --all-features --locked` using an authorized
crates.io token. Do not rerun the workspace publish after any version has been
published: Cargo rejects existing versions. If both crates were published but
the GitHub release step failed, rebuild the archives with
`cargo package --workspace --all-features --locked` and finish with
`gh release create rust-vX.Y.Z target/package/*.crate --generate-notes --title rust-vX.Y.Z --verify-tag`.
Inspect an existing GitHub release before uploading any missing assets.

Ruby gems have a separate version and use `ruby-vX.Y.Z` tags. See the
[Ruby release procedure](bindings/ruby/README.md#releasing-to-rubygems) for
Trusted Publishing setup, precompiled packages, and release validation.
