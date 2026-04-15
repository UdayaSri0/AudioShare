# SynchroSonic

SynchroSonic is a Linux-first desktop audio streaming and casting application
for capturing system audio and sending it to other devices over Wi-Fi/LAN, while
optionally keeping playback active on the sender.

The repository now includes a working GTK4/libadwaita desktop application, a
Linux PipeWire capture and playback backend, mDNS LAN device discovery,
sender-side casting, receiver-mode playback, synchronization diagnostics,
configuration persistence, Linux Bluetooth output targeting as a local sink
choice, and first-pass Linux release metadata and packaging layouts.

Current release posture:

- First public tag target: `v0.1.0-rc.1`, to be published as a GitHub pre-release.
- Native Linux release builds are supported.
- The repository stages real native install, AppDir, and Debian filesystem layouts.
- Final AppImage generation, Debian dependency metadata, signing, and a published private security-reporting path are not fully in place yet.

## Goals

- Provide a modern Linux-native desktop GUI.
- Capture Linux system/output audio through a PipeWire backend.
- Stream audio to receivers on the local network.
- Support simultaneous local playback on the sender.
- Keep discovery, transport, audio, receiver, configuration, and UI code
  separated.
- Keep Linux-specific audio integration behind portable core interfaces so
  future Windows support remains possible.

## Non-Goals For The Current Phase

- No Bluetooth transport or pairing support yet.
- No final signed AppImage or dependency-complete `.deb` installer yet.
- No Windows or macOS audio backend yet.

## Repository Layout

- `crates/synchrosonic-app`: GTK4/libadwaita desktop application shell.
- `crates/synchrosonic-core`: domain models, configuration, diagnostics, shared
  state, and service traits.
- `crates/synchrosonic-audio`: Linux PipeWire-backed source enumeration and raw
  capture frame production.
- `crates/synchrosonic-discovery`: mDNS service advertisement, browsing, and
  in-memory device registry.
- `crates/synchrosonic-transport`: LAN transport session model.
- `crates/synchrosonic-receiver`: receiver-mode runtime boundary.
- `docs/architecture.md`: current architecture overview.
- [docs/quick-start.md](docs/quick-start.md): one simple guide to run the app from source or local build files.
- [docs/developer/README.md](docs/developer/README.md): developer onboarding, local workflow, and CI-aligned checks.
- `docs/roadmap.md`: phase-by-phase implementation roadmap.
- `docs/configuration.md`: config schema, persistence, logging, and recovery.
- `docs/linux-packaging.md`: current Linux packaging assets and remaining gaps.
- `docs/release-checklist.md`: pre-release and publication checklist.
- `docs/adr/`: architecture decision records.

## Development Setup

If you want the shortest possible setup and run guide, start with
[docs/quick-start.md](docs/quick-start.md).

Detailed contributor setup and workflow docs now live in
[docs/developer/README.md](docs/developer/README.md).

Install Rust and the GTK/libadwaita development packages for your distro. On
Ubuntu-like systems, the native dependencies are typically:

```bash
sudo apt install build-essential pkg-config libgtk-4-dev libadwaita-1-dev
```

The current runtime implementation also expects the PipeWire command-line tools
to be available:

```bash
sudo apt install pipewire-bin
```

Useful development commands:

```bash
cargo fmt --all
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
cargo build --release -p synchrosonic-app
bash scripts/package-linux.sh
RUST_LOG=debug cargo run -p synchrosonic-app
RUST_LOG=synchrosonic_audio=debug cargo run -p synchrosonic-audio --example capture_probe
RUST_LOG=synchrosonic_discovery=debug cargo run -p synchrosonic-discovery --example discovery_probe
```

## Packaging And Release

Linux release assets now included in the repository:

- desktop entry: `packaging/linux/org.synchrosonic.SynchroSonic.desktop`
- AppStream metadata: `packaging/linux/org.synchrosonic.SynchroSonic.metainfo.xml`
- scalable icon: `packaging/linux/org.synchrosonic.SynchroSonic.svg`
- staging script: `scripts/package-linux.sh`
- packaging guide: `docs/linux-packaging.md`
- release checklist: `docs/release-checklist.md`
- changelog: `CHANGELOG.md`

The packaging script produces:

- native Linux install layout tarball
- AppDir tarball for later AppImage generation
- Debian filesystem layout tarball

These are real staging outputs, not pretend final installers. The remaining
packaging gaps are documented in `docs/linux-packaging.md`.

For the first public tag, those staged artifacts should be presented as preview
assets for `v0.1.0-rc.1`, not as final signed installers.

## Community

- Contributing guide: `CONTRIBUTING.md`
- Security policy: `SECURITY.md`
- Issue templates: `.github/ISSUE_TEMPLATE/`

## Screenshots

Real screenshots will be added once the UI stabilizes across the current Linux
packaging targets.

Planned captures:

- Dashboard / home
- Discovered devices
- Active casting sessions
- Audio routing
- Receiver mode
- Diagnostics and settings

## License

GPL-3.0-or-later. A `LICENSE` file is present at the repository root.
