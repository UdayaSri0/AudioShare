# Environment Setup

## Supported Development OS

SynchroSonic is currently a Linux-first project.

- Linux development is the only workflow implemented in the repository today.
- GitHub Actions runs on `ubuntu-latest`.
- Windows and macOS development support are not currently available in this
  repo.

## Required Rust Toolchain

The workspace root declares:

- edition: `2021`
- minimum Rust version: `1.85`

CI installs the stable toolchain plus `rustfmt` and `clippy`, so that is the
closest match for local development too.

Install and select a suitable toolchain:

```bash
rustup toolchain install stable --component rustfmt --component clippy
rustup override set stable
rustc --version
cargo --version
```

There is no `rust-toolchain.toml` file in the repo right now, so your local
toolchain is managed through `rustup` plus the workspace `rust-version`
constraint.

## System Dependencies

On Ubuntu-like systems, the repo’s documented and CI-backed package set is:

```bash
sudo apt update
sudo apt install build-essential pkg-config libgtk-4-dev libadwaita-1-dev desktop-file-utils pipewire-bin
```

What each dependency is used for:

- `build-essential`: C toolchain support for native Rust dependencies
- `pkg-config`: native library discovery during builds
- `libgtk-4-dev`: GTK4 headers and pkg-config metadata
- `libadwaita-1-dev`: libadwaita headers and pkg-config metadata
- `desktop-file-utils`: `desktop-file-validate` for packaging validation
- `pipewire-bin`: provides the PipeWire CLI tools used at runtime

The repo does not currently maintain distro-specific install commands beyond
the Ubuntu-style package names above. On non-Debian Linux distributions, install
the equivalent GTK4, libadwaita, pkg-config, and PipeWire CLI packages.

## Required Runtime Tools

The current Linux audio implementation shells out to these commands:

- `pw-dump`
- `pw-record`
- `pw-play`

Those tools must be available on `PATH` when you run the app, the audio example,
or playback/receiver flows.

## Packaging Tooling

If you work on packaging or release assets, these tools are relevant:

- `desktop-file-validate`
- `appstreamcli`

`desktop-file-validate` is installed in CI via `desktop-file-utils`.
`appstreamcli` is optional in the current script: packaging still works when it
is missing, but local AppStream validation is skipped.

## Optional Tools In This Repo

These are not required for every contributor, but they are useful:

- `RUST_LOG=...` for targeted tracing output
- `cargo run -p synchrosonic-audio --example capture_probe`
- `cargo run -p synchrosonic-discovery --example discovery_probe`
- `bash scripts/package-linux.sh` for local packaging layout staging

## Verify The Environment

Use these checks before you start changing code:

```bash
rustc --version
cargo fmt --version
cargo clippy --version
pkg-config --modversion gtk4
pkg-config --modversion libadwaita-1
command -v pw-dump
command -v pw-record
command -v pw-play
```

Then confirm the workspace compiles:

```bash
cargo check --workspace
```

If you only want to confirm the desktop app entrypoint and native GTK
dependencies, use:

```bash
cargo check -p synchrosonic-app
```
