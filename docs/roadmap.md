# SynchroSonic Roadmap

## Phase 1: Project Foundation

- [x] Create Rust workspace.
- [x] Add GTK4/libadwaita application shell.
- [x] Add core models, configuration, diagnostics, state, and service traits.
- [x] Add architecture documentation and first ADR.
- [x] Add compile/test/format baseline.

## Phase 2: Linux Audio Capture

- [x] Choose and document the initial PipeWire integration strategy.
- [x] Enumerate PipeWire sources and outputs through the Linux audio backend.
- [x] Surface missing PipeWire tool/process errors through typed audio errors.
- [x] Add capture session start/stop lifecycle and debug stats hook.
- [ ] Wire capture start/stop to end-to-end sender session controls.
- [ ] Revisit direct PipeWire bindings once system headers/API strategy are settled.

## Phase 3: LAN Discovery

- [x] Choose and document the mDNS crate and service naming scheme.
- [x] Announce sender/receiver services with versioned TXT metadata.
- [x] Discover compatible devices and maintain an in-memory registry.
- [x] Add device list state and diagnostics path.
- [ ] Add richer GUI controls for filtering/selecting discovered receivers.

## Phase 4: Receiver Mode And Playback

- [ ] Add receiver runtime startup path.
- [ ] Add playback sink selection.
- [ ] Add receiver diagnostics and shutdown behavior.

## Phase 5: Sender To Receiver Streaming

- [ ] Design and document the stream framing protocol.
- [ ] Implement sender transport sessions.
- [ ] Implement receiver transport sessions.
- [ ] Add latency/quality settings with tested defaults.

## Phase 6: Simultaneous Local Playback

- [ ] Add local playback mirror mode.
- [ ] Validate PipeWire routing behavior.
- [ ] Document limitations and troubleshooting.

## Phase 7: GUI Completion

- [ ] Wire dashboard controls to real sessions.
- [ ] Add device discovery page.
- [ ] Add audio source/output selection.
- [ ] Add settings, diagnostics, and about details.

## Phase 8: Packaging And Release

- [ ] Decide Flatpak and distro packaging strategy.
- [ ] Add CI checks.
- [ ] Add release documentation.

## Deferred: Bluetooth Output Support

- [x] Model Bluetooth as an output/backend capability.
- [x] Avoid adding Bluetooth transport behavior to the LAN MVP.
- [x] Document receiver-side Bluetooth sink setup once supported.
