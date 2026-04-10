# SynchroSonic

SynchroSonic is a Linux-first desktop audio streaming and casting application
for capturing system audio and sending it to other devices over Wi-Fi/LAN, while
optionally keeping playback active on the sender.

This repository is currently in the early implementation phase. The code builds
a GTK4/libadwaita desktop application, a Linux PipeWire capture and playback
backend, mDNS LAN device discovery, sender-side casting, receiver-mode
playback, synchronization diagnostics, and Linux Bluetooth output targeting as a
local sink choice.

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
- No packaging or installer flow yet.
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
- `docs/roadmap.md`: phase-by-phase implementation roadmap.
- `docs/adr/`: architecture decision records.

## Development Setup

Install Rust and the GTK/libadwaita development packages for your distro. On
Ubuntu-like systems, the native dependencies are typically:

```bash
sudo apt install build-essential pkg-config libgtk-4-dev libadwaita-1-dev
```

Useful development commands:

```bash
cargo fmt --all
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
RUST_LOG=debug cargo run -p synchrosonic-app
RUST_LOG=synchrosonic_audio=debug cargo run -p synchrosonic-audio --example capture_probe
RUST_LOG=synchrosonic_discovery=debug cargo run -p synchrosonic-discovery --example discovery_probe
```

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

GPL-3.0-or-later.
