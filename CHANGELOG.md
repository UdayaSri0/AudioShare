# Changelog

All notable changes to SynchroSonic will be documented in this file.

## [0.1.10] - 2026-04-16

### Added

- Added documented packaging targets for RPM, Arch Linux, Snap, and signed APT repository publication through a separate GitHub Pages workflow.

### Changed

- Refreshed release-facing docs, issue templates, and AppStream metadata so the public `0.1.10` line matches the current packaging outputs and canonical AudioShare repository links.
- Tightened packaging guidance around preview-only paths such as Flatpak runtime validation and manual signed APT publication.
- Updated the portable tarball docs to match the deterministic top-level archive layout now produced by the packaging scripts.

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
