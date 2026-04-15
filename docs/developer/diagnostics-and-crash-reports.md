# Diagnostics And Crash Reports

SynchroSonic keeps runtime diagnostics under the existing XDG state root.
With the default Linux paths, the layout is:

```text
~/.local/state/synchrosonic/
├── app-log.jsonl
└── diagnostics/
    ├── session-marker.json
    ├── subsystem-snapshot.json
    ├── crash-reports/
    └── bundles/
```

If you override the state location with `SYNCHROSONIC_STATE_DIR`, the same
layout is created under that directory.

## What Lives Where

- `app-log.jsonl`: structured JSON-lines application log
- `diagnostics/session-marker.json`: open-session marker used to detect
  abnormal termination on the next launch
- `diagnostics/subsystem-snapshot.json`: periodically persisted snapshot of
  discovery, receiver, streaming, audio inventory, and UI state
- `diagnostics/crash-reports/`: panic reports and abnormal-termination recovery
  reports
- `diagnostics/bundles/`: exported `.tar.gz` diagnostic bundles for debugging
  and issue reports

## How Crash Recovery Works

On startup, the app writes a session marker containing version, PID, OS/kernel,
config path, and state path.

On a graceful shutdown, the marker is removed.

If the next launch finds a stale marker, SynchroSonic writes an abnormal
termination recovery report into `diagnostics/crash-reports/` using:

- the stale session marker
- the last persisted subsystem snapshot
- the current redacted config summary
- the most recent structured log entries from `app-log.jsonl`

This lets us recover useful context after panics, segfaults, forced kills, or
other unclean exits.

## Panic Reports

A panic hook is installed during startup.
When a panic happens, SynchroSonic writes a dedicated crash report containing:

- panic message and source location
- thread name
- a forced Rust backtrace
- the last known subsystem snapshot
- a safe config summary
- recent structured logs

## Collecting Diagnostics From The UI

The Diagnostics page includes working actions for:

- copying a compact human-readable diagnostics summary
- exporting a diagnostic bundle
- opening the diagnostics folder
- opening the crash reports folder
- copying the latest crash report

The exported bundle is a `.tar.gz` archive created in-process so contributors do
not need external archiving tools installed.

## Developer Workflow

To inspect diagnostics locally:

```bash
ls -R ~/.local/state/synchrosonic/diagnostics
sed -n '1,40p' ~/.local/state/synchrosonic/app-log.jsonl
ls -t ~/.local/state/synchrosonic/diagnostics/crash-reports | head
```

To isolate a repro in a temporary state directory:

```bash
mkdir -p /tmp/synchrosonic-dev/config /tmp/synchrosonic-dev/state
SYNCHROSONIC_CONFIG_DIR=/tmp/synchrosonic-dev/config \
SYNCHROSONIC_STATE_DIR=/tmp/synchrosonic-dev/state \
cargo run -p synchrosonic-app
```
