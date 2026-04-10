# Receiver Mode

SynchroSonic receiver mode is now implemented as a dedicated runtime in
`crates/synchrosonic-receiver` and is fed by the TCP listener in
`crates/synchrosonic-transport`. It can be started and stopped from the GTK
app, tracks an explicit receiver lifecycle, buffers inbound PCM packets before
playback, and writes decoded audio to PipeWire on Linux through `pw-play`.

## Runtime Lifecycle

The receiver runtime reports these states through `ReceiverSnapshot`:

- `Idle`: receiver mode is not running.
- `Listening`: receiver mode is started and ready to accept a transport session.
- `Connected`: a transport session is attached and playback resources are open.
- `Buffering`: packets are arriving, but the buffer has not reached the
  configured start threshold or it underrun and is refilling.
- `Playing`: buffered packets are being drained into the playback engine.
- `Error`: the runtime hit a playback/config/transport error and cleared the
  active session.

`AppState` now stores the full `ReceiverSnapshot`, so GTK can render lifecycle,
buffer status, transport/session details, and metrics without owning the
receiver worker thread.

## Transport Contract

Receiver mode stays transport-agnostic internally. The current
`LanReceiverTransportServer` hands events into
`ReceiverRuntime::submit_transport_event` using the shared types in
`crates/synchrosonic-core/src/receiver.rs`.

The contract is:

- `Connected(ReceiverConnectionInfo)`: starts a new inbound session and opens
  the playback sink for the announced stream format.
- `AudioPacket(ReceiverAudioPacket)`: carries one PCM payload chunk plus a
  sequence number, sender media timestamp, and sender wall-clock reference.
- `KeepAlive`: refreshes the transport activity timer.
- `Disconnected { reason, reconnect_suggested }`: tears down playback, clears
  the buffer, and returns the runtime to `Listening`.
- `Error { message }`: moves the runtime to `Error`.

`ReceiverConnectionInfo` includes:

- `session_id`
- `remote_addr`
- `ReceiverStreamConfig { sample_rate_hz, channels, sample_format, frames_per_packet }`
- `requested_latency_ms`

`ReceiverAudioPacket` carries raw PCM bytes. The receiver validates that packet
payload size aligns with the negotiated stream format before buffering it.

## Buffer Management

Receiver buffering is explicit and lives in `buffer.rs` as
`ReceiverPacketBuffer`.

- Packets are stored in a `VecDeque`.
- The runtime does not begin playback until the queued audio reaches the
  preset's target buffer window in milliseconds.
- If the buffer is full, the oldest packet is dropped so latency does not grow
  without bound. This increments the overrun counter.
- If playback needs a packet and the buffer is empty, the runtime records an
  underrun, switches back to `Buffering`, and waits for the buffer to refill.
- If a packet misses its sender-timestamp-based write deadline by more than the
  preset allows, the runtime drops that stale packet instead of playing it late
  and drifting farther away from the sender timeline.

Latency presets now define:

- `playback_latency_ms`: the `pw-play` output buffer target
- `target_buffer_ms`: the receiver-side jitter buffer target
- `max_buffer_ms`: the hard ceiling for queued audio
- `latency_tolerance_ms`: how much scheduling drift is still considered healthy
- `late_packet_drop_ms`: when the receiver should drop instead of playing late
- `reconnect_grace_period`

The current presets are:

- `LowLatency`: about 80 ms expected output latency
- `Balanced`: about 150 ms expected output latency
- `Stable`: about 230 ms expected output latency

These values are centralized in `ReceiverLatencyPreset::profile()` so the UI,
transport diagnostics, and runtime all report the same policy.

## Playback Engine

Linux playback is implemented in `playback.rs` behind a small trait boundary:

- `PlaybackEngine`: creates a playback stream for a negotiated format.
- `PlaybackSink`: accepts PCM bytes and owns the lifecycle of the underlying
  playback process.

`LinuxPlaybackEngine` launches:

```text
pw-play --raw --rate <hz> --channels <n> --format <s16|f32> --latency <preset-ms> [-target <sink>] -
```

That keeps Linux-specific playback logic out of the receiver worker loop while
still making the runtime testable with a mock playback engine.

## Frame Handoff Pipeline

Incoming frames are handed to playback like this:

1. The TCP listener emits `ReceiverTransportEvent::Connected` with the
   negotiated `ReceiverStreamConfig`.
2. The receiver runtime opens a `PlaybackSink` for that stream and transitions
   to `Connected`.
3. Each `AudioPacket` is validated and pushed into `ReceiverPacketBuffer`.
4. The first packet of a sync window anchors a local `Instant` to the sender's
   `captured_at_ms` media clock.
5. Once the buffer reaches the preset target window, the runtime computes a
   write deadline for the oldest queued packet using the sender timestamp plus
   the configured buffer target.
6. The worker pops one buffered packet at a time and writes its PCM payload into
   `pw-play` through `PlaybackSink::write`.
7. If a packet is already far past that deadline, it is dropped as stale so the
   receiver can recover closer to the sender timeline.
8. Successful drains move the runtime to `Playing` and update metrics.
9. Disconnects, timeouts, or playback errors stop the sink, clear the buffer,
   and surface the new state back through `ReceiverSnapshot`.

## Metrics And Logging

`ReceiverSnapshot::metrics` exposes:

- `packets_received`
- `frames_received`
- `bytes_received`
- `packets_played`
- `frames_played`
- `bytes_played`
- `buffer_fill_percent`
- `underruns`
- `overruns`
- `reconnect_attempts`

`ReceiverSnapshot::sync` exposes:

- `state`
- `requested_latency_ms`
- `expected_output_latency_ms`
- `queued_audio_ms`
- `buffer_delta_ms`
- `schedule_error_ms`
- `late_packet_drops`
- `sync_resets`

The runtime also logs:

- session connect/disconnect events
- buffer overrun and underrun events
- playback startup errors
- periodic packet/buffer playback debug information

## App Integration

The GTK app now:

- enables receiver mode in the in-memory app config for the current prototype
- stores `ReceiverSnapshot` inside `AppState`
- provides `Start Receiver` and `Stop Receiver` controls on the Receiver page
- polls the runtime snapshot once per second to refresh the UI and app
  diagnostics

The current LAN streaming path already does exactly that: TCP transport decodes
the wire protocol, emits `ReceiverTransportEvent` values, and the receiver
runtime handles buffering and playback.
