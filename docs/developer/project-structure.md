# Project Structure

## Workspace Overview

The repository is a Rust workspace defined in the root
[`Cargo.toml`](../../Cargo.toml). The current
workspace members are:

- `crates/synchrosonic-app`
- `crates/synchrosonic-audio`
- `crates/synchrosonic-core`
- `crates/synchrosonic-discovery`
- `crates/synchrosonic-receiver`
- `crates/synchrosonic-transport`

## Important Top-Level Folders

- `crates/`: all Rust workspace members
- `docs/`: product, architecture, packaging, and contributor-facing docs
- `docs/developer/`: developer setup and workflow docs
- `.github/workflows/`: CI definitions
- `scripts/`: repository automation scripts
- `packaging/linux/`: desktop metadata and Linux packaging assets
- `target/`: Cargo build outputs and staged packaging artifacts

## Main Crates

### `crates/synchrosonic-app`

GTK4/libadwaita desktop application entrypoint and UI shell.

Important files:

- `src/main.rs`: app startup and GTK application activation
- `src/ui.rs`: main UI, control wiring, background polling, and runtime glue
- `src/persistence.rs`: config/log paths, startup config loading, import/export
- `src/logging.rs`: structured logging and in-memory log store
- `src/metadata.rs`: app id, binary name, version, and support metadata

### `crates/synchrosonic-core`

Shared domain types and configuration used across the workspace.

Important modules:

- `audio.rs`: audio settings, frame model, and capture stats
- `config.rs`: persisted app config and defaults
- `diagnostics.rs`: user-facing diagnostic events
- `error.rs`: shared error types
- `models.rs`: discovered devices, transport endpoints, playback targets
- `receiver.rs`: receiver transport and sync models
- `services.rs`: service traits used by runtime crates
- `state.rs`: application state model
- `streaming.rs`: sender/target/session snapshot types

### `crates/synchrosonic-audio`

Linux audio backend boundary.

Important files:

- `src/linux.rs`: PipeWire source enumeration and capture via `pw-dump` and
  `pw-record`
- `src/playback.rs`: PipeWire playback via `pw-play`
- `examples/capture_probe.rs`: small CLI probe for capture troubleshooting

### `crates/synchrosonic-discovery`

mDNS advertisement, browsing, and device registry.

Important files:

- `src/lib.rs`: discovery service implementation and registry logic
- `examples/discovery_probe.rs`: CLI probe for discovery troubleshooting

### `crates/synchrosonic-receiver`

Receiver-side runtime boundary.

Important files:

- `src/service.rs`: receiver runtime lifecycle
- `src/buffer.rs`: explicit packet buffering behavior
- `src/playback.rs`: playback sink integration for receiver mode

### `crates/synchrosonic-transport`

LAN transport and sender-side fan-out logic.

Important files:

- `src/protocol.rs`: frame protocol and message serialization
- `src/sender.rs`: sender session manager and per-target streaming
- `src/receiver.rs`: receiver transport server
- `src/fanout.rs`: branch queues for local mirror and network targets
- `src/lib.rs`: public exports and transport unit tests

## Where Tests Live

The repo currently uses inline unit tests inside crate source files. There is no
top-level `tests/` directory in the workspace today.

Examples:

- `crates/synchrosonic-app/src/persistence.rs`
- `crates/synchrosonic-core/src/config.rs`
- `crates/synchrosonic-discovery/src/lib.rs`
- `crates/synchrosonic-transport/src/lib.rs`

## Assets, Configs, Packaging, And CI

- Linux desktop assets:
  `packaging/linux/org.synchrosonic.SynchroSonic.desktop`,
  `packaging/linux/org.synchrosonic.SynchroSonic.metainfo.xml`,
  `packaging/linux/org.synchrosonic.SynchroSonic.svg`
- Packaging entrypoint: `scripts/package-linux.sh`
- CI workflow: `.github/workflows/ci.yml`
- Packaging and release docs:
  `docs/linux-packaging.md` and `docs/release-checklist.md`
- Config/log behavior docs: `docs/configuration.md`

## Practical Navigation Tips

- Start in the workspace root for most `cargo` commands.
- Look in `synchrosonic-app` when a change affects the GTK UI or saved settings.
- Look in `synchrosonic-core` first when you need shared config, state, or
  models.
- Look in `synchrosonic-audio` for PipeWire integration issues.
- Look in `synchrosonic-transport` and `synchrosonic-receiver` for sender or
  receiver streaming behavior.
- Look in `.github/workflows/ci.yml` before adding new local verification steps
  so the contributor workflow stays aligned with CI.
