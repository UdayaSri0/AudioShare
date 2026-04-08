# SynchroSonic Architecture

SynchroSonic is organized as a Rust workspace with explicit boundaries between
the desktop UI, portable domain logic, Linux audio integration, LAN discovery,
network transport, and receiver mode.

## Workspace Components

- `synchrosonic-app` owns the GTK4/libadwaita application shell. It renders
  product navigation and reads application state, but does not talk directly to
  PipeWire or network sockets.
- `synchrosonic-core` owns shared domain models, typed configuration,
  diagnostics, state, and service traits used by the other crates.
- `synchrosonic-audio` owns the Linux audio backend boundary. It enumerates
  PipeWire sources/playback targets and exposes raw capture frames through the
  portable audio traits in `synchrosonic-core`.
- `synchrosonic-discovery` owns mDNS service advertisement, browsing, and the
  in-memory registry of SynchroSonic devices seen on the LAN.
- `synchrosonic-transport` owns the LAN streaming session model and will later
  own stream framing, connection lifecycle, fan-out routing, and
  quality/latency controls.
- `synchrosonic-receiver` owns receiver-mode lifecycle, explicit packet
  buffering, transport-event handoff, and playback output.

## Dependency Direction

The intended dependency flow is:

```text
synchrosonic-app
  -> synchrosonic-core
  -> synchrosonic-audio
  -> synchrosonic-discovery
  -> synchrosonic-transport
  -> synchrosonic-receiver

feature crates -> synchrosonic-core
```

Feature crates may depend on `synchrosonic-core`. They should not depend on the
GTK application crate. Linux-specific implementation stays out of
`synchrosonic-core` so portable networking and domain logic can support future
platforms.

## Startup Flow

The app starts logging, creates a default `AppConfig`, builds an `AppState`,
starts discovery, wires the receiver runtime and transport listener, and opens a
GTK/libadwaita window with pages for dashboard, devices, streaming, receiver,
settings, diagnostics, and about.

## Audio Boundary

PipeWire is the preferred Linux audio integration point for source enumeration,
capture, and local mirror playback. The current backend uses `pw-dump` to map
`Audio/Sink` nodes to system-output monitor capture sources and `Audio/Source`
nodes to capture-capable inputs. Capture uses `pw-record` to emit raw PCM frames
through `AudioCapture`.

`AudioFrame` carries sequence, timestamp, format metadata, PCM bytes, and
peak/RMS stats. The application layer can fan these frames out to local
monitoring/playback and the future network streaming encoder without coupling
GTK widgets to PipeWire.

## Discovery And Transport Boundary

mDNS/zeroconf discovery is implemented with `_synchrosonic._tcp.local.` service
advertisement and browsing. Each advertised TXT record includes device identity,
app/protocol version, capabilities, and availability. Discovery events update
`AppState::discovered_devices`; GTK widgets render from app state instead of
owning mDNS sockets.

Transport is modeled separately so the streaming protocol can evolve without
touching GTK UI code. The transport crate now owns TCP session setup,
negotiation, heartbeats, sender-side branch fan-out, per-target session
collection management, and the sender session state snapshot consumed by the UI.

## Bluetooth Scope

Bluetooth support is intentionally deferred. Bluetooth speakers should not be
treated as receiver nodes because they cannot run SynchroSonic receiver code.
Future Bluetooth work should be modeled as an output/backend capability on a
receiver or local device, not as the first LAN transport.
