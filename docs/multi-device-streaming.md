# Multi-Device Sender Streaming

SynchroSonic's sender can now cast one captured audio stream to multiple LAN
receivers at the same time.

## Goal

One sender-side capture session fans out into:

- one optional local playback mirror branch
- one isolated transport branch per active receiver target

The sender session manager keeps these branches separate so a slow or failed
receiver does not collapse the whole cast.

## Session Model

`LanSenderSession` now acts as a persistent sender session manager.

It owns:

- one capture configuration
- one local mirror branch
- a collection of per-target receiver sessions
- a shared snapshot for the GTK/UI layer

The manager can:

- add a new receiver target while other targets are already streaming
- remove one receiver target without stopping the others
- stop all targets and release capture resources
- keep the local mirror toggle independent from the receiver collection

## Per-Target State

Each receiver target exposes:

- receiver identity and endpoint
- connection state
- health state
- session id
- negotiated codec and stream shape
- network buffer fill and drop counters
- per-target bitrate and latency estimate
- last error

This state is surfaced through `StreamTargetSnapshot`.

## Concurrency Model

The sender manager uses one capture loop and several isolated workers:

```text
pw-record
  -> sender manager
     -> local mirror queue -> sender pw-play
     -> target A queue -> target A TCP writer/reader
     -> target B queue -> target B TCP writer/reader
     -> target C queue -> target C TCP writer/reader
```

Receiver connections are established independently. Each target session owns:

- its own TCP socket
- its own writer queue
- its own reader thread
- its own heartbeat tracking
- its own metrics and error state

## Failure Isolation

If one receiver fails:

- its target snapshot moves to `Error`
- its health becomes `Unreachable` or `Error`
- its transport workers are shut down
- the target remains visible in UI state until removed or retried
- other active targets continue receiving frames

If the shared capture path fails, all active targets fail together because they
depend on the same upstream source.

## Buffering Strategy

Every receiver target has an explicit bounded queue. Queue size is derived from:

- negotiated packet duration
- configured target latency
- explicit minimum and maximum packet limits

When a target queue fills:

- the oldest packet is dropped for that target
- that target's drop counter increases
- other targets keep flowing

This keeps one lagging receiver from back-pressuring the entire sender.

## UI Flow

The Streaming page now supports:

- selecting one discovered receiver
- adding that receiver to the active cast
- removing just that receiver
- stopping the whole sender manager
- inspecting per-target state, health, latency, buffer fill, and errors

## End-To-End Pipeline

```text
pw-record
  -> LinuxAudioBackend / AudioCapture
  -> LanSenderSession manager
  -> explicit per-branch fan-out
     -> local mirror queue -> sender pw-play
     -> receiver target queue -> TCP framed transport -> receiver runtime -> receiver pw-play
```
