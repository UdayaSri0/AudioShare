# SynchroSonic Agent Brief

This repository is for a new open-source Linux desktop application that captures
system audio on a sender machine and streams it to other devices over Wi-Fi/LAN,
while optionally continuing playback locally.

Use this document as the source of truth for future coding-agent work in this
repo. If requirements change, update this file and the relevant architecture
decision record before changing implementation direction.

---

## 1. Product Direction

SynchroSonic is a Linux-first desktop GUI application for audio streaming and
casting.

Core user goals:

- Capture desktop/system audio from the Linux sender.
- Stream captured audio to one or more devices on the same Wi-Fi/LAN.
- Allow the sender to keep local playback enabled while casting.
- Provide a polished Linux-native GUI for device discovery, source selection,
  cast controls, diagnostics, and settings.
- Keep Bluetooth support as a later phase, designed cleanly rather than hacked
  into the first LAN streaming path.
- Preserve a modular architecture so future Windows support is realistic.

Expected UX:

- Device discovery view.
- Audio source and output selection.
- Cast start/stop controls.
- Local playback mirror toggle.
- Connected-device status indicators.
- Latency and quality settings.
- Logs/diagnostics page.
- Settings page.
- About page.

---

## 2. Preferred Technical Stack

Default technology choices:

- Rust for the core application and performance-sensitive modules.
- GTK4 plus libadwaita for the Linux desktop GUI.
- PipeWire for Linux audio capture and local playback integration.
- Tokio or equivalent async runtime for networking and background tasks.
- Serde for configuration and data models.
- mDNS/zeroconf for LAN device discovery.
- GStreamer only if it is truly needed and the decision is justified in
  `docs/adr/`.

Architecture constraints:

- Keep UI, audio engine, transport, discovery, and device management separate.
- Keep the transport/streaming layer independent from GTK and Linux-only UI code.
- Keep Linux-specific audio implementation behind traits/interfaces so a future
  Windows backend can be added without rewriting the network core.
- Do not couple LAN streaming to Bluetooth assumptions. Model Bluetooth as a
  future device-output backend or receiver-side sink, not as the first transport.

---

## 3. Non-Negotiable Engineering Rules

General rules:

- Inspect the repository before changing code.
- Do not rewrite working parts unnecessarily.
- Do not produce placeholder architecture with fake implementations.
- Do not remove features unless the reason is clearly justified in comments and
  documentation.
- Keep code idiomatic, strongly typed, and production-oriented.
- Add logging, error handling, and sensible defaults.
- Add tests where practical.
- Add clear TODO markers only for genuinely deferred work.
- Prefer small, reviewable commits.
- Never claim a feature is complete unless it is integrated end-to-end.
- Never pretend code was tested if it was not.

Documentation rules:

- Add README updates whenever a major feature is added.
- Document meaningful design decisions in `docs/adr/`.
- Keep architecture documentation aligned with implementation.
- Document deferred Bluetooth work explicitly so it is not accidentally folded
  into the LAN streaming MVP.

Testing rules:

- Run the available format, lint, test, and build commands after changes.
- If a command cannot be run, report exactly why.
- Prefer unit tests for pure core logic and integration tests around transport,
  discovery, and audio-boundary behavior where practical.

---

## 4. Work Protocol For Every Task

For every coding task, follow this order:

1. Inspect the repo first.
2. Explain the current state briefly.
3. State the exact files that will be created or updated.
4. Implement the task.
5. Run available tests, build, format, and lint commands.
6. Summarize what changed.
7. List follow-up risks or next steps.

If the repo is empty or only contains documentation, do not pretend there is an
existing app. Start by establishing the minimal production-grade project
foundation.

---

## 5. Target Architecture

The app should be organized around these boundaries:

- UI layer: GTK4/libadwaita views, widgets, actions, navigation, and user
  feedback only.
- Application layer: cast session orchestration, settings flow, diagnostics, and
  UI-facing state.
- Audio engine: audio source enumeration, PipeWire capture, optional local mirror
  playback, format negotiation, buffering, and audio health reporting.
- Transport layer: LAN streaming protocol, connection lifecycle, backpressure,
  latency/quality configuration, and stream framing.
- Discovery layer: mDNS/zeroconf announcement and discovery of compatible
  receivers.
- Device management: discovered-device model, connection state, capabilities,
  per-device status, latency settings, and future output-backend metadata.
- Configuration layer: strongly typed persisted settings via Serde.
- Platform layer: Linux-specific PipeWire and desktop integration isolated from
  portable core logic.

The implementation should prefer explicit traits and data models over global
state. UI code should depend on application-facing services, not directly on
PipeWire or sockets.

---

## 6. Suggested Initial Repository Shape

When scaffolding begins, prefer a Rust workspace structure similar to this unless
there is a documented reason to choose otherwise:

```text
.
├── Cargo.toml
├── README.md
├── LICENSE
├── docs/
│   ├── architecture.md
│   ├── synchrosonic_agent_prompt.md
│   └── adr/
│       └── 0001-linux-first-rust-gtk-pipewire.md
├── crates/
│   ├── synchrosonic-app/
│   │   └── src/
│   ├── synchrosonic-core/
│   │   └── src/
│   ├── synchrosonic-audio/
│   │   └── src/
│   ├── synchrosonic-transport/
│   │   └── src/
│   └── synchrosonic-discovery/
│       └── src/
└── tests/
```

Recommended crate responsibilities:

- `synchrosonic-app`: GTK4/libadwaita application shell and UI integration.
- `synchrosonic-core`: shared models, typed config, errors, app state, and
  service traits.
- `synchrosonic-audio`: Linux PipeWire backend and future platform audio
  abstractions.
- `synchrosonic-transport`: LAN streaming protocol and async networking.
- `synchrosonic-discovery`: mDNS/zeroconf discovery and announcements.

Do not add crates just to look modular. Add them when the boundary has real code
or an immediately useful public interface.

---

## 7. Early Milestones

Milestone 1: Project foundation

- Create Rust workspace and baseline crates.
- Add README, license, architecture overview, and first ADR.
- Add formatting, linting, and test commands.
- Add typed configuration models with defaults and tests.

Milestone 2: Linux audio capability probe

- Enumerate PipeWire audio sources and output devices.
- Surface errors clearly when PipeWire is unavailable.
- Add tests around parsing and model logic where feasible.
- Document any system dependencies.

Milestone 3: GUI shell

- Add GTK4/libadwaita app shell.
- Add pages for dashboard, devices, settings, diagnostics, and about.
- Wire UI to typed application state without fake streaming behavior.

Milestone 4: LAN discovery and session model

- Add mDNS discovery/announcement for compatible receivers.
- Add device state and connection-status models.
- Add diagnostics/logging for discovery events.

Milestone 5: Audio streaming prototype

- Implement real capture-to-transport flow.
- Add start/stop cast orchestration.
- Add local playback mirror behavior if supported by the audio backend.
- Document latency/quality defaults and current limitations.

Milestone 6: Production hardening

- Add integration tests where practical.
- Harden error handling, shutdown behavior, logging, and settings persistence.
- Update README and architecture docs with verified behavior.

---

## 8. Design Decisions To Document

Create or update an ADR when choosing:

- The LAN streaming protocol and framing strategy.
- Whether GStreamer is introduced and why Rust/PipeWire-only code is insufficient.
- The mDNS crate and service naming scheme.
- The PipeWire Rust binding or FFI strategy.
- The local playback mirror implementation.
- Any cross-platform abstraction that affects future Windows support.
- Any packaging approach such as Flatpak, distro packages, or AppImage.

ADR files should be short and concrete: context, decision, consequences, and
alternatives considered.

---

## 9. Bluetooth Scope

Bluetooth is explicitly deferred from the first LAN streaming milestone.

Do:

- Keep data models flexible enough to describe future output backends.
- Document Bluetooth as a future receiver/output capability.
- Avoid transport assumptions that would make Bluetooth impossible later.

Do not:

- Pretend Bluetooth speakers can run receiver code.
- Mix Bluetooth-specific pairing logic into the LAN streaming MVP.
- Add a fake Bluetooth toggle that is not wired to real behavior.

---

## 10. Quality Bar

Production-oriented code should include:

- Typed errors with useful context.
- Structured logging/tracing around audio, discovery, and transport state.
- Graceful cancellation and shutdown for async tasks.
- Sensible defaults for latency, quality, and network behavior.
- Clear user-facing diagnostics when dependencies or permissions are missing.
- Tests for config, model conversions, protocol framing, and pure state logic.

Avoid:

- Hardcoded paths outside a dedicated config/platform layer.
- UI code that starts raw subprocesses directly.
- Silent fallbacks that hide broken audio or network behavior.
- Large unreviewable rewrites.
- Placeholder functions that make the UI appear more complete than it is.

---

## 11. Current Repository State

As of this brief, the repository may still be documentation-only. Before any
implementation task, verify the current tree and update this section if the
project foundation has already been created.

