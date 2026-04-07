# SynchroSonic

SynchroSonic is a Linux-first desktop audio streaming and casting application
for capturing system audio and sending it to other devices over Wi-Fi/LAN, while
optionally keeping playback active on the sender.

This repository is currently in the project-foundation phase. The code builds a
GTK4/libadwaita application shell and typed Rust module boundaries, but it does
not yet capture, stream, or play back audio.

## Goals

- Provide a modern Linux-native desktop GUI.
- Capture Linux system/output audio through a PipeWire backend.
- Stream audio to receivers on the local network.
- Support simultaneous local playback on the sender.
- Keep discovery, transport, audio, receiver, configuration, and UI code
  separated.
- Keep Linux-specific audio integration behind portable core interfaces so
  future Windows support remains possible.

## Non-Goals For The Initial Scaffold

- No real audio capture yet.
- No LAN streaming protocol implementation yet.
- No mDNS service registration yet.
- No Bluetooth transport or pairing support yet.
- No packaging or installer flow yet.

## Repository Layout

- `crates/synchrosonic-app`: GTK4/libadwaita desktop application shell.
- `crates/synchrosonic-core`: domain models, configuration, diagnostics, shared
  state, and service traits.
- `crates/synchrosonic-audio`: Linux audio backend boundary for future PipeWire
  capture/playback work.
- `crates/synchrosonic-discovery`: discovery model and mDNS service metadata.
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
cargo run -p synchrosonic-app
```

## License

GPL-3.0-or-later.

