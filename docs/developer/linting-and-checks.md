# Linting And Checks

## Core Commands

Format the workspace:

```bash
cargo fmt --all
```

Check formatting without rewriting files:

```bash
cargo fmt --all --check
```

Run Clippy with the same warning policy CI uses:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Run the full test suite:

```bash
cargo test --workspace
```

Build the release desktop binary:

```bash
cargo build --release -p synchrosonic-app
```

Stage Linux packaging layouts after a release build:

```bash
bash scripts/package-linux.sh --skip-build
```

## Recommended Pre-PR Verification Flow

For normal Rust code changes:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

For UI, packaging, or release-oriented changes, add:

```bash
cargo build --release -p synchrosonic-app
bash scripts/package-linux.sh --skip-build
```

If you want to verify the app still launches after a change, do that manually in
a graphical Linux session:

```bash
RUST_LOG=debug cargo run -p synchrosonic-app
```

## What CI Enforces Today

`.github/workflows/ci.yml` currently enforces:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

The packaging job then runs:

- `cargo build --release -p synchrosonic-app`
- `bash scripts/package-linux.sh --skip-build`

## Common Fixes

Formatting failures:

- run `cargo fmt --all`
- review the rewritten files
- rerun `cargo fmt --all --check`

Clippy failures:

- treat every warning as a blocking failure, because CI passes `-D warnings`
- fix the code and rerun the full Clippy command above
- there is no repo-specific `clippy.toml` in this checkout today

Packaging validation notes:

- the packaging script opportunistically runs `desktop-file-validate`
- it also runs `appstreamcli validate --no-net` when `appstreamcli` is present
- missing optional validators do not currently make the script fail

