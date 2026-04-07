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
- `synchrosonic-audio` owns the Linux audio backend boundary. The initial
  backend reports that real PipeWire enumeration/capture is not active yet
  rather than inventing fake sources.
- `synchrosonic-discovery` owns mDNS service metadata and will later own LAN
  receiver discovery and sender announcements.
- `synchrosonic-transport` owns the LAN streaming session model and will later
  own stream framing, connection lifecycle, and quality/latency controls.
- `synchrosonic-receiver` owns receiver-mode startup state and will later connect
  discovery, transport input, and playback output.

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

The baseline app starts logging, creates a default `AppConfig`, builds an
`AppState`, and opens a GTK/libadwaita window with pages for dashboard, devices,
settings, diagnostics, and about. Controls that would imply real streaming are
disabled until the audio and transport milestones implement real behavior.

## Audio Boundary

PipeWire is the preferred Linux audio integration point for source enumeration,
capture, and local mirror playback. The current scaffold exposes a
`AudioBackend` trait and a `LinuxAudioBackend` type. The backend returns a typed
unavailable error for source/output enumeration until the PipeWire milestone
implements real discovery.

## Discovery And Transport Boundary

mDNS/zeroconf discovery will be used for LAN receiver discovery. Transport is
modeled separately so the streaming protocol can evolve without touching GTK UI
code. The current transport crate tracks session state but does not open network
connections yet.

## Bluetooth Scope

Bluetooth support is intentionally deferred. Bluetooth speakers should not be
treated as receiver nodes because they cannot run SynchroSonic receiver code.
Future Bluetooth work should be modeled as an output/backend capability on a
receiver or local device, not as the first LAN transport.

