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
- final AppImage generation is now automated through `scripts/build-appimage.sh`

Remaining gap:

- signing is not yet implemented for the `.AppImage`
- the runtime still depends on host PipeWire tooling for the current audio backend

The AppDir is now a real input to a final `synchrosonic-<version>-x86_64.AppImage`
build, rather than only a staging artifact.

## Debian Package Plan

Current status:

- the repo stages a Debian-style filesystem tree
- a basic `DEBIAN/control` file is generated during packaging

Remaining gap:

- signing and repository publication are not implemented yet
- CI now builds a real `.deb`, but install-time validation of host dependency
  coverage should continue to be reviewed

This repository now builds a final Debian package with `dpkg-deb --build`, using
`dpkg-shlibdeps` to infer runtime dependencies from the release binary.

## Flatpak Plan

Current status:

- a Flatpak manifest is now version controlled at
  `packaging/flatpak/org.synchrosonic.SynchroSonic.yml`
- the repository includes `scripts/build-flatpak.sh` to build a local Flatpak
  repository and export a `.flatpak` bundle

Remaining gap:

- the current audio backend depends on PipeWire command-line tools that are
  still a host-integration detail for a Flatpak sandbox
- runtime behavior should be validated on a target host because access to
  `pw-dump`, `pw-record`, and `pw-play` is not guaranteed inside every runtime
  environment

Flatpak support is treated as a preview artifact path for desktop users and
for downstream packaging experimentation, with host runtime permissions
clearly documented.

## CI Packaging Scope

The GitHub Actions workflow does the following on Ubuntu:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo build --release -p synchrosonic-app`
- `bash scripts/package-linux.sh --skip-build`

A new tag-triggered release workflow additionally builds and publishes:

- final `synchrosonic-<version>-x86_64.AppImage`
- `synchrosonic_<version>_amd64.deb`
- `synchrosonic-<version>.flatpak`
- `synchrosonic-<version>-linux-x86_64.tar.gz`
- `SHA256SUMS.txt`

The staging workflow remains useful for layout inspection, while the release
workflow produces the final installable artifacts.

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
