# Local Playback Mirror And Split-Stream Fan-Out

SynchroSonic's sender can now keep playing audio locally while the same captured
stream is being cast to a LAN receiver.

## Goal

One captured PipeWire stream fans out into two explicit branches:

- network sender branch
- local playback mirror branch

The fan-out is implemented inside `LanSenderSession`, not in the UI or audio
backend, so we keep branch ownership and buffering policy in one place.

## Branch Design

The sender session builds a single `AudioCapture` stream and turns each captured
chunk into a shared fan-out frame shape:

```text
pw-record
  -> AudioCapture frame
  -> explicit sender fan-out
     -> bounded network queue
     -> bounded local mirror queue
```

Each branch has its own queue and worker:

- network branch:
  - owns TCP audio writes and heartbeat/stop messages
  - can stall or reconnect without blocking local playback writes
- local mirror branch:
  - owns sender-side `pw-play`
  - can be enabled or disabled during an active cast session
  - failures stay local to the mirror branch and do not tear down the network
    stream by themselves

## Buffering Strategy

Both branch queues are bounded. Capacity is derived from negotiated packet
duration plus the configured target latency, then clamped to explicit minimum
and maximum packet counts.

When a branch queue fills:

- the oldest buffered packet is dropped first
- dropped-packet counters are incremented
- the other branch keeps flowing

This keeps the capture loop from blocking on a slower branch while still
preserving recent audio for low-latency playback.

## Runtime Toggle

`AppState::set_local_playback_enabled` stores the desired mirror setting in the
typed config and streaming snapshot.

During an active cast:

1. the GTK toggle updates `AppState`
2. the UI forwards the change to `LanSenderSession`
3. the sender runtime starts or stops the local mirror worker
4. the streaming snapshot reports `Starting`, `Mirroring`, `Stopping`,
   `Disabled`, or `Error`

## Visible Diagnostics

`StreamSessionSnapshot` now exposes:

- `network_buffer`
- `local_mirror.desired_enabled`
- `local_mirror.state`
- `local_mirror.buffer`
- `local_mirror.packets_played`
- `local_mirror.bytes_played`
- `local_mirror.last_error`

The GTK streaming page and dashboard render these values so branch health is
visible without reading logs.

## End-To-End Pipeline

With local mirroring enabled, the sender pipeline is:

```text
pw-record
  -> LinuxAudioBackend / AudioCapture
  -> LanSenderSession
  -> explicit fan-out
     -> bounded network queue
        -> TCP framed transport
        -> receiver runtime buffer
        -> receiver pw-play
     -> bounded local mirror queue
        -> sender pw-play
```

Incoming receiver frames are still handled exactly once on the receiver side:

- TCP `Audio` frames become `ReceiverTransportEvent::AudioPacket`
- the receiver runtime buffers them explicitly
- the receiver playback engine drains those buffered packets into `pw-play`
