# Changelog

All notable changes to SynchroSonic will be documented in this file.

## [0.1.9] - 2026-04-16

### Fixed

- Completed the Linux release pipeline so tagged builds verify and publish real AppImage, Debian, Flatpak, tarball, and checksum artifacts.
- Kept the Debian packaging flow on the proper source-style `debian/control` plus generated `DEBIAN/control` path for final `.deb` generation.
- Aligned application, crate, packaging, and documentation versioning on `0.1.9` and release tag `v0.1.9`.
- Added explicit release artifact verification before GitHub publishing and kept root `Cargo.toml` as the version source of truth for tagged releases.
- Corrected About page, repository links, issue links, maintainer metadata, and release notes to match the canonical AudioShare repository owned by `UdayaSri0`.

### Changed

- Diagnostic reports and exported support metadata continue to include canonical repository ownership and release URLs.
- Packaging docs now describe the real automation state for AppImage, Debian, Flatpak, tarball, and APT-repository scaffold outputs more honestly.
