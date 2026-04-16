# Linux Packaging And Release Assets

This document describes the current Linux packaging state for SynchroSonic
after the `0.1.10` release-engineering pass.

## What Exists In The Repo Now

Linux release metadata and packaging assets live here:

- `packaging/linux/org.synchrosonic.SynchroSonic.desktop`
- `packaging/linux/org.synchrosonic.SynchroSonic.metainfo.xml`
- `packaging/linux/org.synchrosonic.SynchroSonic.svg`
- `packaging/linux/AppRun`
- `packaging/flatpak/org.synchrosonic.SynchroSonic.yml`
- `packaging/arch/PKGBUILD`
- `packaging/rpm/synchrosonic.spec.in`
- `snap/snapcraft.yaml`
- `debian/control`
- `debian/changelog`
- `scripts/package-linux.sh`
- `scripts/build-appimage.sh`
- `scripts/build-deb.sh`
- `scripts/build-flatpak.sh`
- `scripts/build-arch-package.sh`
- `scripts/build-rpm.sh`
- `scripts/build-release-artifacts.sh`
- `scripts/verify-release-artifacts.sh`
- `scripts/build-apt-repo.sh`
- `.github/workflows/publish-apt-repository.yml`

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
- `RELEASE_MANIFEST.json`

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
- `rpm`
- `rpmbuild`
- `makepkg` on Arch Linux hosts
- `snapcraft` on a Snap-capable Ubuntu/Linux build environment

Runtime assumptions in the current implementation:

- `pw-dump`
- `pw-record`
- `pw-play`
- a PipeWire session that exposes sources and playback sinks

The Debian package metadata now includes `pipewire-bin` explicitly because the
current backend shells out to PipeWire CLI tools instead of linking against a
library API directly.

The RPM package path uses file-based dependencies on `/usr/bin/pw-dump`,
`/usr/bin/pw-record`, and `/usr/bin/pw-play` so RPM-based distributions can
resolve the real provider package for the PipeWire CLI tools instead of relying
on a guessed package name.

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

The portable tarball is now deterministic and extracts into a single top-level
directory named `synchrosonic-<version>-linux-<arch>/` instead of unpacking
directly into the current directory. That top-level directory includes a short
`README.txt` plus the staged `usr/` layout.

If you are on a Fedora/openSUSE/RHEL-family machine with RPM tooling installed,
you can build the RPM artifact separately:

```bash
bash scripts/build-rpm.sh --skip-build
```

Or let the RPM script trigger the release build before packaging:

```bash
bash scripts/build-rpm.sh
```

If you are on an Arch Linux host and want a local `makepkg` build from the
current working tree, run:

```bash
bash scripts/build-arch-package.sh
```

If you want to build the Snap locally from the repository root on a machine
with Snapcraft available, run:

```bash
snapcraft
```

For a disposable CI or container-style environment where destructive builds are
acceptable, you can also use:

```bash
snapcraft --destructive-mode
```

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
7. validate the finished archive with `dpkg-deb --info` and `dpkg-deb --contents`

This fixes the previous failure where `dpkg-shlibdeps` tried to parse a binary
`DEBIAN/control` file as if it were a source-style `debian/control`.

The Debian build script now also:

- validates the staged binary, desktop file, icon, metainfo, and packaged docs
- checks that `debian/control` starts with `Source:` and still contains a
  package stanza
- checks that `debian/changelog` matches the workspace version before building
- fails with explicit messages if any Debian packaging command is missing or
  returns an error

## AppImage Status

Current status:

- the repo produces a valid AppDir-style directory
- `AppRun`, desktop entry, icon, binary, and AppStream metadata are staged together
- README and LICENSE are staged into the AppDir documentation tree
- final AppImage generation is automated through `scripts/build-appimage.sh`
- `appimagetool` is cached under `target/tools/` so repeat builds do not
  re-download it unless the cache is missing
- the AppImage script validates the staged AppDir inputs before invoking
  `appimagetool`, including the staged desktop files, AppStream metadata,
  binary, launcher, icon, README, and LICENSE

Remaining gap:

- signing is not yet implemented for the `.AppImage`
- runtime still depends on host PipeWire tooling for the current audio backend

## Portable Tarball Status

Current status:

- the portable archive is named `synchrosonic-<version>-linux-<arch>.tar.gz`
- the archive is written deterministically with stable ordering, normalized
  ownership, and reproducible gzip output
- extracting it creates a single `synchrosonic-<version>-linux-<arch>/`
  directory containing the staged `usr/` layout
- the portable archive includes a root-level `README.txt` for quick inspection
- `SHA256SUMS.txt` now lists each published artifact by filename only instead of
  embedding absolute paths
- `RELEASE_MANIFEST.json` captures artifact kind, filename, size, and `sha256`
  from the same source of truth that writes `SHA256SUMS.txt`

## Flatpak Status

Current status:

- the Flatpak manifest is version controlled
- `scripts/build-flatpak.sh` builds a local Flatpak repository and exported bundle
- native Flatpak builds are supported in CI and on developer hosts with Flatpak tooling
- local release builds can fall back to a Docker-based Flatpak builder image when host tooling is missing
- the Docker fallback reuses the cached `synchrosonic-flatpak-builder:24.04`
  image unless `SYNCHROSONIC_FLATPAK_REBUILD_DOCKER_IMAGE=1` is set
- the first native or Docker-backed Flatpak build downloads the Freedesktop
  runtime and SDK into `target/flatpak-home/`, so the initial run can take a
  while before the bundle export step
- the fallback is only used when `flatpak` or `flatpak-builder` is missing;
  a failing native Flatpak build now fails directly instead of silently retrying
  under Docker
- the finished bundle is written to
  `target/release-packaging/synchrosonic-<version>.flatpak`
- the builder now installs explicit `org.freedesktop.Platform/x86_64/24.08`
  and `org.freedesktop.Sdk/x86_64/24.08` refs and exports the app bundle from
  the manifest's `stable` branch instead of relying on Flatpak defaults
- the manifest keeps only the finish-args the current app path actually uses:
  network access, desktop/audio sockets, GPU access, and `/run/user` access for
  the current PipeWire-integrated runtime preview

Remaining gap:

- the current audio backend depends on PipeWire command-line tools that are
  still a host-integration detail for a Flatpak sandbox
- runtime validation should continue on target hosts because access to
  `pw-dump`, `pw-record`, and `pw-play` is runtime/environment dependent

Flatpak support should therefore be described as an automated preview artifact
path, not as a fully sandbox-independent runtime guarantee.

## RPM Packaging Flow

The RPM path now stages the install tree with the existing `package-linux.sh`
flow and then wraps it with `rpmbuild` using a generated spec file:

- `packaging/rpm/synchrosonic.spec.in` is the template used for the final spec
- `scripts/build-rpm.sh` renders the template with the workspace version,
  release number, source archive name, and host RPM architecture
- the source archive is the staged native install tarball that already contains
  the binary, desktop file, icon, metainfo, README, LICENSE, and packaging docs
- `rpmbuild -bb` emits a final RPM under `target/release-packaging/`
- the script validates the result with `rpm -qip` and `rpm -qlp`

Current output naming:

- `synchrosonic-<version>-1.<arch>.rpm`

Current workflow posture:

- the RPM builder is wired for local or dedicated RPM-capable CI environments
- the main GitHub release workflow does not require or publish RPM assets yet
- `scripts/verify-release-artifacts.sh --require-rpm` is available for a future
  workflow step without changing the current release job behavior

## Snap Packaging Flow

The Snap packaging path is now version-controlled and intended for local or
dedicated CI builds, but it is not wired into store publication:

- `snap/snapcraft.yaml` defines a `core24` strict-confinement desktop snap
- the app uses the GNOME extension for GTK/libadwaita desktop integration
- the build path compiles `synchrosonic-app` from source and installs the
  desktop file, icon, metainfo, README, LICENSE, and packaging docs into the
  snap
- the snap bundles `pipewire-bin` so the app can find `pw-dump`, `pw-record`,
  and `pw-play` inside confinement instead of assuming host binaries are on
  `PATH`
- the app plug list requests `network`, `network-bind`, `audio-playback`,
  `audio-record`, and `pipewire`

To build and install locally:

```bash
snapcraft
sudo snap install --dangerous ./synchrosonic_0.1.10_amd64.snap
```

Current Snap viability and caveats:

- this is an optional packaging target, not an automatic Snap Store publish path
- the main release workflow does not build or publish `.snap` artifacts yet
- `audio-record` is not auto-connected by default, so capture-oriented features
  may require manual connection or store review before they work for end users
- `pipewire` is also not auto-connected by default, which makes receiver/capture
  runtime behavior more sensitive to manual interface connection and target-host
  testing
- the current backend still shells out to PipeWire CLI tools, so the Snap path
  should be treated as build-ready but runtime-preview until stricter
  confinement testing is completed
- this repository does not contain Snap Store credentials or automatic publish
  automation

## APT Repository Publication Path

The repo now includes a local APT scaffold generator plus a separate signed
GitHub Pages publication workflow:

- `scripts/build-apt-repo.sh`

After `scripts/build-release-artifacts.sh` finishes, the scaffold lives under:

- `target/release-packaging/apt-repo/`

It currently generates:

- `pool/` with the built `.deb`
- `dists/stable/main/binary-<arch>/Packages`
- `dists/stable/main/binary-<arch>/Packages.gz`
- `dists/stable/Release`
- `index.html`
- `README.txt`
- `.nojekyll`

When invoked with `--sign`, the same builder also generates:

- `dists/stable/InRelease`
- `dists/stable/Release.gpg`
- `keyrings/synchrosonic-archive-keyring.gpg`
- `keyrings/synchrosonic-archive-keyring.asc`
- `install/synchrosonic.sources`

Current publication posture:

- the default `build-release-artifacts.sh` path still produces the unsigned
  local scaffold only
- signed publication is handled by the separate
  `.github/workflows/publish-apt-repository.yml` workflow
- that workflow is manual and secret-gated so the main release flow stays stable
- repository signing depends on `APT_GPG_PRIVATE_KEY_BASE64` and
  `APT_GPG_PASSPHRASE`
- the `.deb` release asset remains the fallback Debian/Ubuntu install path if
  Pages or secrets are not configured yet

## CI Packaging Scope

The pull request / push workflow stages packaging layouts after passing Rust
format, lint, and test checks.

The tagged release workflow validates version/tag consistency and then builds:

- AppImage
- Debian `.deb`
- Flatpak bundle
- portable tarball
- checksum manifest
- local `RELEASE_MANIFEST.json`
- verifies that the required artifacts exist before publishing the GitHub release

RPM packaging is staged for the next release-workflow expansion once an
RPM-capable build job is added.

Snap packaging is similarly staged as an optional local/CI target until the
runtime confinement story is proven well enough for automated release builds or
store publication.

The signed APT repository path is now automatable through GitHub Pages, but it
remains a separate manual workflow rather than part of the main tagged release
publish job.

## Arch Linux Packaging Flow

The Arch packaging path is intentionally local-first and AUR-ready, but it does
not pretend to publish to the AUR automatically:

- `packaging/arch/PKGBUILD` builds SynchroSonic from source with `cargo`
- `scripts/build-arch-package.sh` reads the current workspace version from
  `Cargo.toml`, verifies that `pkgver` matches it, creates a source tarball
  from the current working tree, copies the `PKGBUILD`, and runs `makepkg -f`
- the package installs the app binary, desktop file, icon, metainfo, README,
  LICENSE, and `linux-packaging.md`

This package builds from the local source tree rather than a prebuilt binary.
That choice fits Arch packaging norms better, keeps the package reproducible on
an Arch host, and makes the `PKGBUILD` a reasonable starting point for a future
AUR submission.

Current workflow posture:

- Arch packaging is available as a documented local packaging target
- the main GitHub release workflow does not build or publish Arch packages
- there is no automatic AUR publication in this repository

## Remaining Release Gaps

The main blockers before calling packaging fully polished are:

- signing is not yet implemented for AppImage or standalone Debian release assets
- signed APT publication remains a separate manual workflow because it depends on GitHub Pages and signing secrets
- Flatpak runtime behavior still depends on host access to PipeWire CLI tools
- the root `LICENSE` file is a short-form GPL notice rather than the full text
- release pages still need real screenshots
