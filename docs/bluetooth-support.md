# Bluetooth Output Support

SynchroSonic now supports Bluetooth as a selected local playback output on
Linux, without changing the main LAN streaming architecture.

## What Bluetooth Means In This Version

Bluetooth is not a transport path and it is not a receiver node.

In the current version, Bluetooth support means:

- the Linux audio backend enumerates PipeWire playback sinks and classifies
  Bluetooth-capable outputs
- the sender-side local mirror branch can target a selected Bluetooth sink
- receiver-mode playback can target a selected Bluetooth sink
- the app remembers the selected sink id and keeps that selection visible even
  if the sink temporarily disappears
- the UI surfaces availability changes and Bluetooth classification in
  diagnostics and status views

The network architecture remains:

```text
LAN sender -> TCP transport -> receiver runtime -> local playback sink
```

Bluetooth only affects the final local playback sink on the machine running
SynchroSonic.

## Linux Implementation

Bluetooth output detection lives in `synchrosonic-audio`.

PipeWire playback targets are classified as Bluetooth when the sink metadata
indicates BlueZ/BT characteristics, for example:

- `device.bus=bluetooth`
- `device.api` or `factory.name` references BlueZ
- `api.bluez5.address` is present
- the sink name starts with `bluez_output.`

Each playback target is exposed through the shared `PlaybackTarget` model with:

- `kind`: `Standard` or `Bluetooth`
- `availability`
- optional `bluetooth_address`

## Usage Notes

For the sender local mirror:

1. Open the Streaming page.
2. Enable local mirroring if desired.
3. Choose a playback output from the local mirror output selector.
4. Start or continue casting.

For receiver mode:

1. Open the Receiver page.
2. Choose a playback output from the receiver output selector.
3. Start receiver mode or let the new target apply to the next playback change.

Selecting the system default output keeps `pw-play` unpinned and lets PipeWire
choose the current default sink. Selecting a specific Bluetooth sink pins
playback to that PipeWire target id.

## Disconnect And Reconnect Behavior

The app polls playback targets and reports when a selected output becomes
unavailable or available again.

Current behavior:

- selections are preserved when a Bluetooth sink disappears
- the selector keeps showing the chosen sink as unavailable instead of clearing
  it silently
- the local mirror branch retries automatically if the selected sink comes back
  after the mirror entered an error state
- receiver-mode playback keeps the configured target for later sessions and
  reports the availability change clearly

## Limitations

This is intentionally a controlled phase, not a full Bluetooth session manager.

- SynchroSonic does not pair Bluetooth devices itself; PipeWire/BlueZ must
  already expose the sink on the system
- active receiver playback is not guaranteed to resume seamlessly if the
  Bluetooth sink disappears mid-stream
- Bluetooth latency is outside SynchroSonic's direct control and can be much
  higher or more variable than wired sinks
- LAN transport and Bluetooth output timing are still separate layers, so using
  Bluetooth on one receiver can increase end-to-end skew relative to wired
  receivers
- device-specific codec or profile controls are not exposed in the app yet

## Recommended Expectations

- Use wired outputs when you need the tightest multi-room sync
- Use Bluetooth outputs when convenience matters more than lowest possible skew
- Treat Bluetooth as a local output choice on a sender/receiver machine, not as
  an alternative to SynchroSonic's LAN receiver architecture
