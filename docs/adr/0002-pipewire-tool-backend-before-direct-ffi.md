# ADR 0002: PipeWire Tool Backend Before Direct FFI

## Status

Accepted.

## Context

The Linux audio phase needs real PipeWire-first enumeration and capture, but this
development environment does not expose `libpipewire-0.3` through `pkg-config`.
Adding a direct PipeWire FFI crate now would make the workspace fail to compile
locally.

## Decision

Implement the first Linux capture backend using PipeWire tools:

- `pw-dump` for source and playback-target enumeration.
- `pw-record` for raw PCM capture.

Keep this behind the portable `AudioBackend` and `AudioCapture` traits in
`synchrosonic-core`, so the backend can be replaced with a direct PipeWire API
implementation later without changing GTK widgets or transport consumers.

## Consequences

- The project compiles in the current environment without PipeWire development
  headers.
- Enumeration and capture are real PipeWire behavior, not fake in-memory data.
- Runtime capture depends on PipeWire command-line tools being available.
- Error handling must surface missing command/process failures clearly.
- A future ADR should revisit direct PipeWire bindings when development headers
  and API requirements are settled.

## Alternatives Considered

- Direct PipeWire Rust bindings now: better long-term control, but not buildable
  in this environment without additional system packages.
- PulseAudio compatibility tools: broader fallback, but not PipeWire-first.
- Fake source enumeration: rejected because it would hide integration risk.

