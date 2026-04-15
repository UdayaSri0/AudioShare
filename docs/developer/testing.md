# Testing

## Run Tests From The Workspace Root

The standard test entrypoint for this repo is:

```bash
cargo test --workspace
```

That matches the test step in `.github/workflows/ci.yml`.

## Common Test Commands

Full workspace tests:

```bash
cargo test --workspace
```

Run one crate’s tests:

```bash
cargo test -p synchrosonic-transport
```

Run one crate’s library unit tests only:

```bash
cargo test -p synchrosonic-transport --lib
```

Run a single named test:

```bash
cargo test -p synchrosonic-transport sender_can_stream_to_multiple_targets_and_remove_one_without_stopping_the_other
```

Run a single named test with stdout/stderr visible:

```bash
cargo test -p synchrosonic-transport sender_can_stream_to_multiple_targets_and_remove_one_without_stopping_the_other -- --nocapture
```

Run one crate with verbose test output:

```bash
cargo test -p synchrosonic-transport --lib -- --nocapture
```

## Flag Reference

- `--workspace`: run tests for every workspace member
- `-p <package>`: scope the run to a single crate
- `--lib`: run the package’s library unit tests; useful because this repo keeps
  most tests inline in `src/*.rs`
- `-- --nocapture`: forward flags to the Rust test harness so prints/logs are
  shown

## Reproducing CI Failures Locally

The current CI workflow has two relevant verification stages:

Lint-and-test job:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Packaging job:

```bash
cargo build --release -p synchrosonic-app
bash scripts/package-linux.sh --skip-build
```

If CI fails, run the same commands in the same order from the repo root before
digging deeper.

## Timing-Sensitive Test Debugging

The transport crate contains timing-sensitive streaming tests. When one of those
fails, debug it with narrow local runs before rerunning the full workspace:

```bash
cargo test -p synchrosonic-transport --lib -- --nocapture
```

If you need to shake out a flake, rerun just the target test several times:

```bash
for i in {1..10}; do cargo test -p synchrosonic-transport sender_can_stream_to_multiple_targets_and_remove_one_without_stopping_the_other -- --nocapture || break; done
```

When adjusting timing-related tests in this repo, prefer deterministic wait or
poll helpers over longer fixed sleeps.

