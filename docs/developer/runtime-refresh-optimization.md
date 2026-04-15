# Runtime Refresh Optimization

This page captures the prompt 3 runtime cleanup work in the GTK app so future
transport and browser-join work has a quieter baseline.

## Why This Changed

Before this pass, the desktop app did more work than it needed while idle:

- startup performed a synchronous PipeWire inventory enumeration on the GTK
  activation path
- audio inventory updates had already been slowed to `8s`, but the initial load
  still blocked the window coming up
- `refresh_ui()` could rebuild multiple page lists even when their row content
  had not changed
- duplicate refresh attempts still showed up as noisy debug output
- browser-join prototype state was not part of the UI refresh diff, so that
  state could be missed by later refreshes

The result was unnecessary GTK churn, noisy logs, and slower startup than the
current architecture needs.

## What Changed

### Startup

Before:

- `build_main_window()` called a full audio inventory refresh synchronously
  before the window was shown

After:

- the initial audio inventory refresh is spawned in the background immediately
  after the UI starts
- the result pump applies the inventory back onto the GTK thread when ready
- the window can present first, then fill in sources/outputs shortly after

### Audio Inventory

Before:

- startup inventory work ran inline on the UI path

After:

- audio inventory stays on the existing `8s` cadence when idle
- the first refresh is immediate and asynchronous
- overlapping inventory jobs are rejected cleanly and logged as skipped
- unchanged inventory results no longer force list rebuilds

### UI Rendering

Before:

- list-heavy pages could clear and rebuild GTK rows even when the visible data
  had not changed

After:

- discovery rows are diffed before rebuilding
- casting target rows are diffed before rebuilding
- audio source rows are diffed before rebuilding
- playback target rows are diffed before rebuilding
- diagnostics event rows are diffed before rebuilding
- structured log rows are diffed before rebuilding
- duplicate `refresh_ui()` calls are skipped when state, recent logs, visible
  view, and browser-join snapshot are unchanged

## Idle Churn: Before vs After

These are the practical idle-mode differences that matter most.

| Area | Before | After |
| --- | --- | --- |
| Window activation | synchronous inventory work on startup path | background inventory load after the window appears |
| Audio enumeration cadence | async polling already reduced, but startup still blocked | immediate async load, then idle refresh every `8s` |
| GTK list rendering | repeated list rebuilds on unrelated refreshes | diff-based row rendering; unchanged lists are skipped |
| Duplicate refresh noise | duplicate refresh attempts still logged loudly | duplicate refreshes tracked, but quiet at `trace` unless something changed |
| Browser-join UI state | not part of diff key | included in refresh diff so honest prototype state is preserved |

## CPU Churn Notes

This pass intentionally uses lightweight churn proxies instead of adding a new
allocator or profiler dependency.

The app now records and/or logs:

- poll interval
- poll duration
- skipped overlapping polls
- applied change count per poll
- UI refreshes requested vs applied vs skipped
- list rebuilds vs list skips
- last rendered row count

Those numbers live in the subsystem snapshot and the diagnostics summary. They
are the recommended idle-mode health signals for contributors.

In practical terms, idle CPU churn is lower because:

- no full PipeWire inventory call runs on the GTK startup path
- no full audio enumeration runs every second
- unchanged lists no longer rebuild rows
- no-op refreshes are skipped earlier and logged more quietly

## Logs To Watch

Useful structured log messages now include:

- poll interval and duration
- whether a refresh was skipped because the previous run was still active
- how many changes were actually applied
- whether a UI list was rebuilt or intentionally skipped

A quiet idle app should mostly show:

- discovery, receiver, and streaming polls with `0` applied changes
- audio inventory refresh every `8s`
- very few list rebuild messages after the initial settle period

## Developer Workflow

1. Run the app with `RUST_LOG=debug cargo run -p synchrosonic-app`.
2. Let it idle on the dashboard and then on the audio, devices, and diagnostics
   pages.
3. Use the Diagnostics page or the persisted subsystem snapshot to confirm:
   - applied refreshes stay low while idle
   - skipped refreshes increase when nothing is changing
   - list rebuilds stop increasing rapidly after initial population
   - audio source and playback target polls stay on the `8s` cadence

## Follow-Up Opportunities

This pass keeps the architecture intact. Reasonable next steps include:

- diffing combo-box option models so selector contents also avoid rebuilds when
  unrelated state changes
- moving more discovery-triggered UI work to event-only paths
- exposing these churn counters in a small developer-facing diagnostics section
  inside the UI
