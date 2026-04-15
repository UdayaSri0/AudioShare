# Linux Packaging Gap Summary

This summary documents the Linux packaging state discovered before the release
pipeline changes were applied. It captures what already existed and what was
still missing before the final artifact automation was added.

This repository already contains real Linux packaging staging support for SynchroSonic:

- Rust workspace with desktop app and shared crates.
- Packaging metadata at `packaging/linux/`:
  - `org.synchrosonic.SynchroSonic.desktop`
  - `org.synchrosonic.SynchroSonic.metainfo.xml`
  - `org.synchrosonic.SynchroSonic.svg`
  - `AppRun`
- `scripts/package-linux.sh` stages:
  - native Linux install layout
  - AppDir layout for AppImage generation
  - Debian filesystem layout with a generated `DEBIAN/control`
- CI uploads staging tarballs from `target/release-packaging/`.

## What is missing for AppImage

- a pinned AppImage toolchain to produce a final `.AppImage`
- local script to generate and validate the final AppImage artifact
- CI step to build and publish `synchrosonic-<version>-x86_64.AppImage`

## What is missing for Debian

- a real `.deb` build step, not only a staging layout tarball
- dependency metadata based on the built ELF binary
- final package naming and CI artifact generation
- smoke checks for `dpkg-deb --info`/`--contents`

## What is missing for Flatpak

- a version-controlled Flatpak manifest
- a reproducible Flatpak build and exported `.flatpak` bundle
- documented runtime permissions for PipeWire and network discovery
- a consistent release artifact strategy for Flatpak output

## What CI/release changes are needed

- keep existing PR/push CI for fmt/clippy/tests/package staging
- add a tag-triggered release workflow for final packaging
- validate tag/version consistency on release tags
- build AppImage, Debian package, Flatpak bundle, portable tarball
- generate `SHA256SUMS.txt` for published artifacts
- upload release assets to GitHub from the final workflow
