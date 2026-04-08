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
  sequence number and capture timestamp.
- `KeepAlive`: refreshes the transport activity timer.
- `Disconnected { reason, reconnect_suggested }`: tears down playback, clears
  the buffer, and returns the runtime to `Listening`.
- `Error { message }`: moves the runtime to `Error`.

`ReceiverConnectionInfo` includes:

- `session_id`
- `remote_addr`
- `ReceiverStreamConfig { sample_rate_hz, channels, sample_format, frames_per_packet }`

`ReceiverAudioPacket` carries raw PCM bytes. The receiver validates that packet
payload size aligns with the negotiated stream format before buffering it.

## Buffer Management

Receiver buffering is explicit and lives in `buffer.rs` as
`ReceiverPacketBuffer`.

- Packets are stored in a `VecDeque`.
- The runtime does not begin playback until the buffer reaches the preset's
  start threshold.
- If the buffer is full, the oldest packet is dropped so latency does not grow
  without bound. This increments the overrun counter.
- If playback needs a packet and the buffer is empty, the runtime records an
  underrun, switches back to `Buffering`, and waits for the buffer to refill.

Latency presets define the playback latency, start threshold, max packet depth,
and reconnect grace period:

- `LowLatency`: 60 ms playback latency, start after 2 packets, max 6 packets
- `Balanced`: 120 ms playback latency, start after 4 packets, max 10 packets
- `Stable`: 180 ms playback latency, start after 6 packets, max 16 packets

These values are centralized in `ReceiverLatencyPreset::profile()` so Prompt 5
and later sync work can reuse the same policy.

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
4. Once the buffer reaches the preset threshold, the runtime transitions to
   `Buffering` and schedules timed packet drains based on
   `frames_per_packet / sample_rate_hz`.
5. The worker pops one buffered packet at a time and writes its PCM payload into
   `pw-play` through `PlaybackSink::write`.
6. Successful drains move the runtime to `Playing` and update metrics.
7. Disconnects, timeouts, or playback errors stop the sink, clear the buffer,
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
