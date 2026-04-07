# ADR 0001: Linux-First Rust Workspace With GTK4, libadwaita, and PipeWire

## Status

Accepted.

## Context

SynchroSonic needs to capture Linux system audio, stream it to LAN receivers,
optionally keep local playback active, and present a polished Linux-native
desktop experience. The codebase also needs boundaries that leave room for a
future Windows implementation.

## Decision

Use a Rust workspace with separate crates for the GTK application, portable core
models/configuration, Linux audio integration, discovery, transport, and receiver
mode. Use GTK4 plus libadwaita for the desktop app, PipeWire as the Linux audio
integration target, Serde for data/config models, and async Rust for future
networking.

The initial scaffold includes compileable module boundaries without implementing
fake audio capture, fake discovery, or fake streaming.

## Consequences

- UI code can evolve independently from transport and audio backends.
- Linux-specific PipeWire code stays outside the portable core.
- Future Windows work can introduce a new audio backend without replacing the
  domain and transport model.
- GTK/libadwaita development packages are required for building the desktop app.
- Additional ADRs are required before choosing the PipeWire binding, mDNS crate,
  stream framing protocol, packaging format, or Bluetooth strategy.

## Alternatives Considered

- Python/PySide: faster prototyping, but weaker fit for the performance-sensitive
  streaming core and current preferred stack.
- Single Rust crate: simpler initially, but likely to blur UI/audio/transport
  boundaries.
- GStreamer-first architecture: powerful, but not justified before validating the
  simpler PipeWire-first design.

