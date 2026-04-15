# Running Locally

## Start From The Repo Root

Most commands in this repo are intended to be run from the workspace root:

```bash
cd /path/to/AudioShare
```

## What Runs Locally

The main developer entrypoint is the GTK4/libadwaita desktop app in the
`synchrosonic-app` crate.

- package name: `synchrosonic-app`
- binary name: `synchrosonic-app`
- application id: `org.synchrosonic.SynchroSonic`

There is no separate backend service to start first. The desktop app creates
its own discovery service, sender session, receiver runtime, and receiver
transport server when it launches.

## Debug Run

For a normal local development run:

```bash
cargo run -p synchrosonic-app
```

To enable debug-level logs on stdout:

```bash
RUST_LOG=debug cargo run -p synchrosonic-app
```

The logging layer honors `RUST_LOG` if it is set. Otherwise it falls back to
`info` or `debug` based on the saved `diagnostics.verbose_logging` setting.

## Release Run

To run the optimized app directly through Cargo:

```bash
cargo run --release -p synchrosonic-app
```

To build first and launch the binary manually:

```bash
cargo build --release -p synchrosonic-app
./target/release/synchrosonic-app
```

## Runtime Expectations

Local app runs currently assume:

- a Linux graphical session that can open GTK4/libadwaita windows
- PipeWire tools on `PATH`: `pw-dump`, `pw-record`, `pw-play`
- a working PipeWire session for capture and playback
- LAN/mDNS availability if you want to discover other SynchroSonic devices

The app does not require a separate database, HTTP service, or message broker.

## First Launch Behavior

On first launch, the app creates config and log files automatically.

Default paths:

- config directory: `~/.config/synchrosonic`
- state directory: `~/.local/state/synchrosonic`
- active config: `~/.config/synchrosonic/config.toml`
- portable export: `~/.config/synchrosonic/config-export.toml`
- structured log: `~/.local/state/synchrosonic/app-log.jsonl`

## Run With Isolated Local State

When you want a clean developer sandbox, point the app at temporary config and
state roots:

```bash
mkdir -p /tmp/synchrosonic-dev/config /tmp/synchrosonic-dev/state
SYNCHROSONIC_CONFIG_DIR=/tmp/synchrosonic-dev/config \
SYNCHROSONIC_STATE_DIR=/tmp/synchrosonic-dev/state \
RUST_LOG=debug \
cargo run -p synchrosonic-app
```

The app still creates its own `synchrosonic/` subdirectory under those base
paths.

## Useful Local Probes

If you want to test individual subsystems without opening the GUI, the repo has
two example programs:

Audio capture probe:

```bash
RUST_LOG=synchrosonic_audio=debug cargo run -p synchrosonic-audio --example capture_probe
```

Discovery probe:

```bash
RUST_LOG=synchrosonic_discovery=debug cargo run -p synchrosonic-discovery --example discovery_probe
```

These are especially useful when the full desktop app launches but audio
capture or mDNS discovery does not behave as expected.

