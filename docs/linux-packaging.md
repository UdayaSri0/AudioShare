# Linux Packaging And Release Assets

This document describes the current Linux packaging state for SynchroSonic
after the `0.1.9` release-engineering pass.

## What Exists In The Repo Now

Linux release metadata and packaging assets live here:

- `packaging/linux/org.synchrosonic.SynchroSonic.desktop`
- `packaging/linux/org.synchrosonic.SynchroSonic.metainfo.xml`
- `packaging/linux/org.synchrosonic.SynchroSonic.svg`
- `packaging/linux/AppRun`
- `packaging/flatpak/org.synchrosonic.SynchroSonic.yml`
- `debian/control`
- `debian/changelog`
- `scripts/package-linux.sh`
- `scripts/build-appimage.sh`
- `scripts/build-deb.sh`
- `scripts/build-flatpak.sh`
- `scripts/build-release-artifacts.sh`
- `scripts/verify-release-artifacts.sh`
- `scripts/build-apt-repo.sh`

The staging script writes three inspection-friendly filesystem layouts under
`target/release-packaging/`:

- native Linux install tree
- AppDir layout for AppImage generation
- Debian filesystem tree with a staged `DEBIAN/control`

The tagged release flow builds the final artifact set:

- `synchrosonic-<version>-x86_64.AppImage`
- `synchrosonic_<version>_amd64.deb`
- `synchrosonic-<version>.flatpak`
- `synchrosonic-<version>-linux-x86_64.tar.gz`
- `SHA256SUMS.txt`

## Build And Runtime Requirements

Build-time requirements:

- Rust toolchain `1.85+`
- `pkg-config`
- `libgtk-4-dev`
- `libadwaita-1-dev`
- `desktop-file-utils`
- `appstream`
- `dpkg-dev`
- `curl`
- `flatpak`
- `flatpak-builder`
- `docker` as an optional local Flatpak fallback when host Flatpak tooling is unavailable

Runtime assumptions in the current implementation:

- `pw-dump`
- `pw-record`
- `pw-play`
- a PipeWire session that exposes sources and playback sinks

The Debian package metadata now includes `pipewire-bin` explicitly because the
current backend shells out to PipeWire CLI tools instead of linking against a
library API directly.

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

If you have the full release packaging toolchain installed and want the final
artifact set locally, run:

```bash
bash scripts/build-release-artifacts.sh --skip-build
```

The release script now expects a final Flatpak bundle as part of the artifact
set. If native `flatpak` and `flatpak-builder` are missing locally, the script
falls back to a Docker-based Flatpak builder image. If neither native tooling
nor Docker is available, the release build fails before checksums or publishing.

## Debian Packaging Flow

The Debian path now distinguishes source metadata from binary package metadata
correctly:

- `debian/control` is the source-style Debian control file used by Debian tooling
- `debian/changelog` carries the package version metadata used by `dpkg-gencontrol`
- `target/release-packaging/deb/DEBIAN/control` is the final binary package
  control file generated for the staged package root

`scripts/build-deb.sh` now performs this sequence:

1. stage the Debian filesystem tree
2. run `dpkg-shlibdeps` against the built release binary
3. write substvars for shared-library dependencies
4. run `dpkg-gencontrol` with `debian/control` and `debian/changelog`
5. generate the final `DEBIAN/control`
6. build the package with `dpkg-deb --build`

This fixes the previous failure where `dpkg-shlibdeps` tried to parse a binary
`DEBIAN/control` file as if it were a source-style `debian/control`.

## AppImage Status

Current status:

- the repo produces a valid AppDir-style directory
- `AppRun`, desktop entry, icon, binary, and AppStream metadata are staged together
- final AppImage generation is automated through `scripts/build-appimage.sh`

Remaining gap:

- signing is not yet implemented for the `.AppImage`
- runtime still depends on host PipeWire tooling for the current audio backend

## Flatpak Status

Current status:

- the Flatpak manifest is version controlled
- `scripts/build-flatpak.sh` builds a local Flatpak repository and exported bundle
- native Flatpak builds are supported in CI and on developer hosts with Flatpak tooling
- local release builds can fall back to a Docker-based Flatpak builder image when host tooling is missing

Remaining gap:

- the current audio backend depends on PipeWire command-line tools that are
  still a host-integration detail for a Flatpak sandbox
- runtime validation should continue on target hosts because access to
  `pw-dump`, `pw-record`, and `pw-play` is runtime/environment dependent

Flatpak support should therefore be described as an automated preview artifact
path, not as a fully sandbox-independent runtime guarantee.

## APT Repository Scaffold

The repo now includes an unsigned APT repository scaffold generator:

- `scripts/build-apt-repo.sh`

After `scripts/build-release-artifacts.sh` finishes, the scaffold lives under:

- `target/release-packaging/apt-repo/`

It currently generates:

- `pool/` with the built `.deb`
- `dists/stable/main/binary-<arch>/Packages`
- `dists/stable/main/binary-<arch>/Packages.gz`
- `dists/stable/Release`

What it does not do yet:

- sign `Release` or `InRelease`
- publish to GitHub Pages or another APT host automatically
- provide repository key management

## CI Packaging Scope

The pull request / push workflow stages packaging layouts after passing Rust
format, lint, and test checks.

The tagged release workflow validates version/tag consistency and then builds:

- AppImage
- Debian `.deb`
- Flatpak bundle
- portable tarball
- checksum manifest
- verifies that the required artifacts exist before publishing the GitHub release

## Remaining Release Gaps

The main blockers before calling packaging fully polished are:

- signing is not yet implemented for AppImage or Debian releases
- APT publication is scaffolded but not automated or signed yet
- Flatpak runtime behavior still depends on host access to PipeWire CLI tools
- the root `LICENSE` file is a short-form GPL notice rather than the full text
- release pages still need real screenshots
