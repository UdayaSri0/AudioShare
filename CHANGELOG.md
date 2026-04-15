# Changelog

All notable changes to SynchroSonic will be documented in this file.

## [0.1.0-rc.2] - 2026-04-15

### Added

- Release packaging automation for AppImage, Debian `.deb`, Flatpak bundle, and portable tarball outputs.
- New helper scripts:` scripts/build-appimage.sh`, `scripts/build-deb.sh`, `scripts/build-flatpak.sh`, and `scripts/build-release-artifacts.sh`.
- Tag-triggered GitHub Actions release workflow that validates version/tag consistency and uploads final release assets.
- Flatpak manifest under version control at `packaging/flatpak/org.synchrosonic.SynchroSonic.yml`.

### Changed

- Updated packaging naming conventions to include version and architecture consistently.
- Release metadata now targets `v0.1.0-rc.2` for the current shipping candidate.

### Known Limitations

- Flatpak support is available as a build artifact, but PipeWire command-line audio integration remains a host-integration limitation.
- Signing is not yet implemented for AppImage or Debian packages.

## [0.1.0-rc.1] - 2026-04-15

First public release candidate for the Linux-first Rust/GTK application.

### Added

- GTK4/libadwaita desktop application shell for discovery, casting, receiver
  mode, diagnostics, settings, and About metadata.
- Linux PipeWire-backed system-audio capture, local playback targeting, and
  receiver playback support through `pw-dump`, `pw-record`, and `pw-play`.
- LAN discovery, sender-to-receiver transport, synchronized multi-target
  casting, and staged Linux packaging layouts for native install trees, AppDir,
  and Debian-style filesystem inspection.

### Changed

- Discovery now suppresses duplicate no-op updates and prefers usable LAN
  endpoints over loopback and common Docker bridge addresses for receiver
  selection.
- Release metadata, README guidance, packaging metadata, and contributor docs
  now describe the first public tag as a pre-release candidate rather than a
  stable release.

### Fixed

- Fixed the GTK startup crash triggered when discovery updated the receiver
  selector and re-entered `RefCell`-backed UI state through a ComboBox callback.

### Known Limitations

- Linux is the only supported runtime for this release candidate.
- Public release artifacts are staging layouts, not final signed AppImage or
  dependency-complete `.deb` installers.
- The repository does not yet publish a dedicated private security reporting
  channel, so a stable public release is not justified yet.
