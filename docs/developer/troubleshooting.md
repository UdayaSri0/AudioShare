# Troubleshooting

## `cargo build` Fails With GTK4 Or libadwaita pkg-config Errors

Symptom:

- build output mentions missing `gtk4` or `libadwaita-1`
- `pkg-config` cannot find native library metadata

Fix on Ubuntu-like systems:

```bash
sudo apt update
sudo apt install build-essential pkg-config libgtk-4-dev libadwaita-1-dev
```

Then retry:

```bash
cargo check -p synchrosonic-app
```

## `pw-dump`, `pw-record`, Or `pw-play` Is Missing

Symptom:

- audio examples fail immediately
- capture/playback features do not start
- runtime errors mention a missing command

Fix:

```bash
sudo apt install pipewire-bin
command -v pw-dump
command -v pw-record
command -v pw-play
```

The current Linux backend shells out to those commands directly, so they must
be present on `PATH`.

## Rust Toolchain Is Too Old

Symptom:

- Cargo reports that the crate requires a newer compiler
- local `rustc` is older than the workspace `rust-version = "1.85"`

Fix:

```bash
rustup update stable
rustup override set stable
rustc --version
```

## The GTK App Does Not Launch In A Headless Shell

Symptom:

- the process cannot open a window
- you are running from a non-graphical environment or CI shell

Fix:

- run the app from a Linux graphical session
- use the lower-level probes when you only need subsystem validation:

```bash
RUST_LOG=synchrosonic_audio=debug cargo run -p synchrosonic-audio --example capture_probe
RUST_LOG=synchrosonic_discovery=debug cargo run -p synchrosonic-discovery --example discovery_probe
```

The repo’s CI does not launch the GUI today; it verifies build, lint, test, and
packaging steps instead.

## Discovery Does Not Find Other Devices

Symptom:

- the app launches, but no peers appear in the Devices view
- the discovery probe stays empty

Fix:

```bash
RUST_LOG=synchrosonic_discovery=debug cargo run -p synchrosonic-discovery --example discovery_probe
```

Then confirm:

- the devices are on the same LAN
- mDNS traffic is allowed on the network
- the app is using the default service type
  `_synchrosonic._tcp.local.`

## Capture Probe Reports No Sources

Symptom:

- `capture_probe` prints `No PipeWire capture sources were found.`
- the app shows no usable monitor/default source

Fix:

- confirm a PipeWire session is active
- confirm the machine exposes monitor or capture-capable sources
- rerun the probe with logs enabled:

```bash
RUST_LOG=synchrosonic_audio=debug cargo run -p synchrosonic-audio --example capture_probe
```

## Receiver Mode Or Transport Bind Fails

Symptom:

- receiver mode does not start
- a bind failure mentions the listen port

Relevant defaults from `synchrosonic-core`:

- `transport.stream_port = 51700`
- `receiver.listen_port = 51700`

Fix:

- stop any other local process already using that port
- check whether another SynchroSonic instance is still running
- if needed, change the saved config and retry

The active config is usually:

```text
~/.config/synchrosonic/config.toml
```

## Saved Config Is Broken Or Keeps Resetting

Symptom:

- the app falls back to defaults on startup
- settings are not restored as expected

What the app does:

- repairs supported invalid values in place
- backs up unusable configs to `config.invalid-<unix-seconds>.toml`
- writes a fresh default config when no saved config exists

Useful paths:

- `~/.config/synchrosonic/config.toml`
- `~/.config/synchrosonic/config-export.toml`
- `~/.local/state/synchrosonic/app-log.jsonl`

You can also isolate state during debugging:

```bash
mkdir -p /tmp/synchrosonic-dev/config /tmp/synchrosonic-dev/state
SYNCHROSONIC_CONFIG_DIR=/tmp/synchrosonic-dev/config \
SYNCHROSONIC_STATE_DIR=/tmp/synchrosonic-dev/state \
cargo run -p synchrosonic-app
```

## A Test Fails In CI But Not Locally

Start by reproducing the exact CI flow:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

If the failure is isolated to one crate or one transport test:

```bash
cargo test -p synchrosonic-transport --lib -- --nocapture
cargo test -p synchrosonic-transport sender_can_stream_to_multiple_targets_and_remove_one_without_stopping_the_other -- --nocapture
```

## Packaging Script Fails Because The Release Binary Is Missing

If you pass `--skip-build`, the script expects this file to already exist:

```text
target/release/synchrosonic-app
```

Fix:

```bash
cargo build --release -p synchrosonic-app
bash scripts/package-linux.sh --skip-build
```

Or let the script do the build:

```bash
bash scripts/package-linux.sh
```

## Desktop Metadata Validation Is Skipped

This is not always a failure. `scripts/package-linux.sh` only runs:

- `desktop-file-validate` when it is installed
- `appstreamcli validate --no-net` when `appstreamcli` is installed

If you need those validations locally, install the missing tools and rerun the
packaging step.

