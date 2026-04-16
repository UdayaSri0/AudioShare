# Changelog

All notable changes to SynchroSonic will be documented in this file.

## [0.1.8] - 2026-04-16

### Fixed

- Fixed the Debian packaging flow so source-style `debian/control` and binary `DEBIAN/control` are no longer confused.
- Replaced the old control-file shortcut with a proper `dpkg-shlibdeps` + `dpkg-gencontrol` path for final `.deb` generation.
- Aligned application, crate, packaging, and documentation versioning on `0.1.8` and release tag `v0.1.8`.
- Updated release validation so the root workspace version from `Cargo.toml` remains the source of truth for tagged releases.
- Corrected About page, repository links, issue links, maintainer metadata, and release notes to match the canonical AudioShare repository owned by `UdayaSri0`.

### Changed

- Diagnostic reports and exported support metadata continue to include canonical repository ownership and release URLs.
- Packaging docs now describe the real automation state for AppImage, Debian, Flatpak, and tarball outputs more honestly.
