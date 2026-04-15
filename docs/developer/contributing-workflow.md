# Contributing Workflow

## Recommended Local Workflow

This repo does not currently document a required branch naming scheme or commit
message format. The practical expectation is focused changes, clear commits, and
local verification before opening a PR.

## 1. Sync Your Local Checkout

Start from the latest main branch state:

```bash
git switch main
git pull --ff-only
```

## 2. Create A Focused Branch

Use a short descriptive branch name for the work you are doing:

```bash
git switch -c <descriptive-branch-name>
```

## 3. Implement The Change

While coding in this repo:

- keep Linux Wi-Fi/LAN streaming as the main architecture
- prefer understandable, debuggable implementations over clever hidden behavior
- avoid broad refactors unless the task needs them
- update docs when behavior, dependencies, packaging, or support promises change

When a change affects validation, parsing, state transitions, or recovery, add
or update tests as part of the same work.

## 4. Run Format, Lint, And Tests

Minimum local verification:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

If formatting fails, run:

```bash
cargo fmt --all
```

## 5. Verify A Local App Launch

For app-facing changes, do a manual launch in a graphical Linux session:

```bash
RUST_LOG=debug cargo run -p synchrosonic-app
```

If the change is narrower, the example probes are often enough:

```bash
RUST_LOG=synchrosonic_audio=debug cargo run -p synchrosonic-audio --example capture_probe
RUST_LOG=synchrosonic_discovery=debug cargo run -p synchrosonic-discovery --example discovery_probe
```

## 6. Run Release-Oriented Checks When Relevant

If you touched packaging assets, desktop metadata, or release docs, also run:

```bash
cargo build --release -p synchrosonic-app
bash scripts/package-linux.sh --skip-build
```

If packaging scope changed, keep these docs in sync in the same branch:

- `docs/linux-packaging.md`
- `docs/release-checklist.md`
- `docs/configuration.md`

## 7. Prepare The PR

Before opening the PR:

- keep the diff focused
- summarize the user-visible behavior change
- call out tradeoffs or limitations honestly
- mention runtime dependency or packaging changes explicitly
- include screenshots only when UI changes are intentional and stable enough to
  review

## 8. Open The PR After Local Verification

The closest local match to CI is:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p synchrosonic-app
bash scripts/package-linux.sh --skip-build
```

You do not need every step for every change, but this is the right full flow for
high-confidence verification before review.

