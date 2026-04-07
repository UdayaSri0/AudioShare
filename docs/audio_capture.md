# Linux Audio Capture

SynchroSonic uses a PipeWire-first Linux audio backend in
`synchrosonic-audio`. The backend is not coupled to GTK widgets; the UI reads
audio state through `synchrosonic-core` traits and app state.

## Enumeration

The Linux backend runs `pw-dump --no-colors` and parses PipeWire node metadata.
It maps:

- `Audio/Sink` nodes to monitor capture sources for system/output audio.
- `Audio/Source` nodes to microphone or capture-capable input sources.
- `Audio/Sink` nodes to playback targets for future local monitoring controls.

Default sink/source metadata is used to mark the default capture choices. The
app prefers the default sink monitor when choosing automatically because
SynchroSonic's primary capture path is system/output audio. Users and future UI
controls can still select another current source through
`AppState::set_audio_sources` and `AppState::select_audio_source`.

## Capture Pipeline

Capture starts through the portable `AudioBackend::start_capture` interface with
`CaptureSettings`. On Linux, `LinuxAudioBackend` starts:

```text
pw-record --target <source-id> --raw --rate <hz> --channels <n> --format <fmt> --latency <ms> -
```

`pw-record` emits raw PCM bytes on stdout. The backend reads stdout on a
background thread and converts each chunk into an `AudioFrame`.

Each frame contains:

- `sequence`: monotonic frame sequence.
- `captured_at`: time elapsed since capture start.
- `sample_rate_hz`, `channels`, and `sample_format`.
- `payload`: raw PCM bytes.
- `stats`: peak and RMS amplitude for diagnostics or a level meter.

The next application layer can fan these frames out to local monitoring/playback
and a network streaming encoder. Buffering is explicit in `CaptureSettings` via
`buffer_frames`, `target_latency_ms`, channel count, and sample format.

## Error Handling

The backend returns typed `AudioError` values for missing PipeWire tools,
`pw-dump` failures, invalid capture settings, process startup failures, and
capture-stream termination. If a device disconnects or `pw-record` stops
emitting data, the capture session marks its stats as `DeviceDisconnected` or
`Failed` and closes the frame stream.

## Developer Diagnostics

When capture starts, the backend logs source ID, sample rate, channel count,
format, latency, buffer size, and enabled outputs. While frames are flowing it
periodically logs peak/RMS frame stats. `AudioCapture::stats()` exposes the same
developer-visible counters and level data to future diagnostics UI.

For a manual local probe, run:

```bash
RUST_LOG=synchrosonic_audio=debug cargo run -p synchrosonic-audio --example capture_probe
```

The probe selects the default PipeWire monitor/source, prints a few frame sizes
and peak/RMS values, then stops capture.
