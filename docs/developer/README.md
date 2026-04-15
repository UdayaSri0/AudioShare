# Developer Documentation

This section is for contributors working on the AudioShare/SynchroSonic Rust
workspace itself: the GTK desktop app, Linux audio backend, discovery,
transport, receiver runtime, packaging, and CI verification flow.

All commands in this folder assume you are starting from the repository root
unless a page says otherwise.

## Who This Is For

- new contributors setting up the project for the first time
- maintainers reproducing CI failures locally
- developers changing Rust crates, packaging assets, or contributor tooling

## Developer Docs

- [Environment Setup](./environment-setup.md)
- [Project Structure](./project-structure.md)
- [Running Locally](./running-locally.md)
- [Testing](./testing.md)
- [Linting And Checks](./linting-and-checks.md)
- [Build And Release](./build-and-release.md)
- [Troubleshooting](./troubleshooting.md)
- [Contributing Workflow](./contributing-workflow.md)

## Quick Start For Developers

1. Install the Rust toolchain and Linux dependencies described in
   [Environment Setup](./environment-setup.md).
2. From the repo root, run the same core checks CI runs:

   ```bash
   cargo fmt --all --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```

3. Launch the desktop app locally:

   ```bash
   RUST_LOG=debug cargo run -p synchrosonic-app
   ```

4. Build the release binary when you need production-like output:

   ```bash
   cargo build --release -p synchrosonic-app
   ```

5. If you are touching packaging assets or release docs, also stage the Linux
   packaging layouts:

   ```bash
   bash scripts/package-linux.sh --skip-build
   ```

