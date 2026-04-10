# Linux Packaging And Release Assets

This document describes the Linux packaging work that now exists in the
repository and the gaps that still need to be closed before a polished public
release.

## What Exists In The Repo Now

Linux release metadata and packaging assets live here:

- `packaging/linux/org.synchrosonic.SynchroSonic.desktop`
- `packaging/linux/org.synchrosonic.SynchroSonic.metainfo.xml`
- `packaging/linux/org.synchrosonic.SynchroSonic.svg`
- `packaging/linux/AppRun`
- `scripts/package-linux.sh`

The packaging script stages three real filesystem layouts under
`target/release-packaging/`:

- native Linux install tree
- AppDir layout for later AppImage generation
- Debian-style filesystem tree with a generated `DEBIAN/control`

It also archives those layouts as tarballs so CI can upload them as build
artifacts.

## Build And Runtime Requirements

Build-time requirements:

- Rust toolchain `1.85+`
- `pkg-config`
- `libgtk-4-dev`
- `libadwaita-1-dev`

Runtime assumptions in the current implementation:

- `pw-dump`
- `pw-record`
- `pw-play`
- a PipeWire session that exposes sources and playback sinks

This means packaging is not just about GTK/libadwaita libraries. Linux users
also need PipeWire command-line tools available at runtime because the current
audio backend shells out to them directly.

## Local Packaging Workflow

Build the release binary and stage packaging layouts:

```bash
cargo build --release -p synchrosonic-app
bash scripts/package-linux.sh --skip-build
```

Or let the script perform the release build first:

```bash
bash scripts/package-linux.sh
```

The script writes outputs to `target/release-packaging/`.

## Native Linux Build Plan

Current status:

- the repo can build `target/release/synchrosonic-app`
- the packaging script stages a native install tree with:
  - `usr/bin/synchrosonic-app`
  - desktop entry
  - AppStream metadata
  - scalable icon
  - README and LICENSE docs

This is enough for distro maintainers or contributors to inspect the install
layout and adapt it to their preferred build system.

## AppImage Plan

Current status:

- the repo produces a valid AppDir-style directory
- `AppRun`, desktop entry, icon, binary, and metadata are staged together

Remaining gap:

- the repository does not yet pin or automate a final AppImage tool such as
  `appimagetool` or `appimage-builder`
- the CI job therefore publishes the AppDir staging artifact, not a final
  `.AppImage`

This is intentional. The AppDir is real and useful, but the final AppImage step
still needs a tool choice, signing policy, and runtime-library strategy.

## Debian Package Plan

Current status:

- the repo stages a Debian-style filesystem tree
- a basic `DEBIAN/control` file is generated during packaging

Remaining gap:

- runtime dependencies are not auto-calculated yet
- maintainer scripts, changelog packaging, signing, and repository publication
  are not automated yet
- CI therefore publishes the Debian layout staging artifact, not a final `.deb`

This keeps the packaging work honest. The filesystem layout is implemented now,
but final Debian packaging still needs dependency and policy refinement.

## CI Packaging Scope

The GitHub Actions workflow does the following on Ubuntu:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo build --release -p synchrosonic-app`
- `bash scripts/package-linux.sh --skip-build`

It uploads the staged packaging tarballs so maintainers can inspect what the
release layout currently looks like without pretending they are final signed
installers.

## Remaining Release Gaps

The main blockers before calling packaging fully release-ready are:

- final AppImage generation and signing are not automated yet
- final Debian dependency metadata is not generated yet
- the root `LICENSE` file is present, but it is still a short-form notice rather
  than the full GPL text some distributors expect
- no screenshot assets are included in the repository yet
- runtime packaging still assumes the host system provides PipeWire command-line
  tools

## Suggested Next Packaging Iteration

- choose and pin one AppImage toolchain
- decide whether Debian packaging lives in `debian/` or remains script-driven
- add dependency verification for GTK/libadwaita/PipeWire runtime pieces
- replace the short-form license notice with the full GPL text if distro
  distribution is a target
- add real screenshots once the UI is stable enough for release pages
