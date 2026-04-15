# Quick Start

This is the shortest guide for running SynchroSonic on Linux.

Use this page if you want one simple document instead of the full developer
documentation set.

## What You Need

SynchroSonic currently supports Linux development and local runs.

Install the required system packages on Ubuntu-like systems:

```bash
sudo apt update
sudo apt install build-essential pkg-config libgtk-4-dev libadwaita-1-dev pipewire-bin
```

Install Rust with `rustup`, then make sure `cargo` is available:

```bash
rustup toolchain install stable
rustup override set stable
rustc --version
cargo --version
```

The app also expects these PipeWire tools on your `PATH`:

```bash
command -v pw-dump
command -v pw-record
command -v pw-play
```

## Option 1: Run From Source Code

Clone the repository, move to the project root, and run the desktop app:

```bash
git clone https://github.com/synchrosonic/synchrosonic.git
cd synchrosonic
cargo run -p synchrosonic-app
```

If you want debug logs on the terminal:

```bash
RUST_LOG=debug cargo run -p synchrosonic-app
```

## Option 2: Build A Release Binary And Run It

Build the optimized app binary:

```bash
cargo build --release -p synchrosonic-app
```

Then start it directly:

```bash
./target/release/synchrosonic-app
```

## Option 3: Run From The Packaged Files Created By This Repo

This repository can stage Linux packaging layouts, but it does not yet produce
final signed installers automatically.

Create the staging files:

```bash
bash scripts/package-linux.sh
```

If you want to open the binary from the staged native layout:

```bash
mkdir -p /tmp/synchrosonic-native
tar -xzf target/release-packaging/*-native-layout.tar.gz -C /tmp/synchrosonic-native
/tmp/synchrosonic-native/usr/bin/synchrosonic-app
```

## First Run Notes

On first launch, the app creates config and log files automatically.

Default locations:

- config: `~/.config/synchrosonic/config.toml`
- log: `~/.local/state/synchrosonic/app-log.jsonl`

## If The App Does Not Start

Check these first:

- you are on Linux with a graphical desktop session
- `pw-dump`, `pw-record`, and `pw-play` are installed
- GTK4 and libadwaita development packages are installed

If you need more detail after this quick guide, use:

- [Developer Docs](./developer/README.md)
- [Running Locally](./developer/running-locally.md)
- [Troubleshooting](./developer/troubleshooting.md)

