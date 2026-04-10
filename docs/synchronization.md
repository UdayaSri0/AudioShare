# Synchronization And Latency Management

This document describes the current first-pass multi-device synchronization
model in SynchroSonic. The goal in v1 is not a perfect distributed clock. The
goal is understandable, debuggable timing that keeps multiple LAN receivers
close enough together for normal home and office listening.

## Current Timing Model

The sender capture loop defines the media timeline.

- `AudioFrame.captured_at` is a monotonic timestamp measured from the start of
  the sender capture session.
- Transport audio frames carry that value as `captured_at_ms`.
- Transport audio frames also carry `captured_at_unix_ms` as an explicit
  sender-side wall-clock reference for diagnostics and future sync work.

The receiver does not try to build a full shared wall clock in v1. Instead it
uses the sender media timeline directly:

1. The first packet of a sync window anchors the sender's `captured_at_ms` to a
   local `Instant`.
2. The receiver builds a jitter buffer until it reaches the preset's
   `target_buffer_ms`.
3. The receiver computes a local write deadline for each packet from:

```text
local_anchor + (packet.captured_at_ms - anchor.captured_at_ms) + target_buffer_ms
```

4. When that deadline arrives, the packet is written into `pw-play`.
5. `pw-play --latency` provides the device/output-side buffer. The expected
   speaker latency is therefore:

```text
target_buffer_ms + playback_latency_ms
```

This keeps every receiver following the same sender media clock even though the
implementation stays simple.

## Presets

The receiver presets live in
`crates/synchrosonic-core/src/receiver.rs::ReceiverLatencyPreset::profile()`.

- `LowLatency`: `30 ms` target buffer + `50 ms` playback buffer = about `80 ms`
  expected output latency
- `Balanced`: `70 ms` target buffer + `80 ms` playback buffer = about `150 ms`
  expected output latency
- `Stable`: `120 ms` target buffer + `110 ms` playback buffer = about `230 ms`
  expected output latency

Each preset also defines:

- `max_buffer_ms`
- `latency_tolerance_ms`
- `late_packet_drop_ms`
- `reconnect_grace_period`

## Receiver Buffer Strategy

The receiver buffer is explicit and bounded.

- Readiness is based on queued audio duration, not just packet count.
- The buffer reports queued packets, queued frames, and queued audio in
  milliseconds.
- If the buffer reaches `max_buffer_ms`, the oldest packet is dropped so the
  receiver does not accumulate unbounded delay.
- If the oldest packet is already too far past its scheduled write deadline, it
  is dropped as stale instead of being played even later.
- If the buffer underruns, the receiver resets its local sync anchor and starts
  a new priming window on the next arriving packet.

## Diagnostics

`ReceiverSnapshot` now exposes timing details through both `buffer` and `sync`.

Buffer diagnostics:

- `queued_audio_ms`
- `target_buffer_ms`
- `max_buffer_ms`
- `fill_percent()`

Sync diagnostics:

- `state`
- `requested_latency_ms`
- `expected_output_latency_ms`
- `queued_audio_ms`
- `buffer_delta_ms`
- `schedule_error_ms`
- `late_packet_drops`
- `sync_resets`
- `last_sender_timestamp_ms`
- `last_sender_capture_unix_ms`

The GTK receiver status view prints these values directly, and the app
diagnostics stream emits warnings when the receiver enters `Late` or
`Recovering`, or when stale packets are dropped.

## Assumptions And Limitations

The current version is intentionally honest about what it does not solve yet.

- It does not run an NTP/PTP-style distributed clock algorithm.
- It assumes that LAN jitter is modest enough that a simple anchored sender
  media clock plus bounded buffer is good enough.
- The local anchor is based on packet arrival timing, so cross-device skew can
  still reflect differences in initial network delay.
- `captured_at_unix_ms` is carried for observability and future improvements,
  but v1 playback scheduling does not depend on sender and receiver system
  clocks matching.
- There is no drift correction beyond packet scheduling and stale-packet drops.
  If sender and receiver clocks drift significantly over long sessions, skew can
  still accumulate.
- TCP head-of-line blocking still applies because the transport is TCP.
- Sender local mirror playback is not sample-locked to remote receivers in this
  version.

## Future Improvements

- Add a startup clock-sampling exchange so receivers can estimate one-way delay
  more accurately.
- Negotiate latency more explicitly between sender request and receiver preset.
- Add long-session drift correction instead of relying only on stale-packet
  drops and buffer resets.
- Expose sync history graphs or counters in a dedicated diagnostics page.
- Evaluate sender-side scheduled playout timestamps once a stronger clock model
  exists.
- Consider UDP or RTP-style transport once loss handling and recovery are ready.
