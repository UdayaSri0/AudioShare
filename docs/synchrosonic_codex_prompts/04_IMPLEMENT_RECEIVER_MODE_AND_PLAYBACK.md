# Prompt 4 — implement receiver mode and local playback engine

```text
Implement receiver mode for the application.

Goal:
A discovered device can act as an audio receiver and play incoming audio cleanly.

Requirements:
- Add receiver service lifecycle:
  - idle
  - listening
  - connected
  - buffering
  - playing
  - error
- Implement a playback engine on Linux
- Build explicit buffer management
- Add basic latency controls or presets
- Handle disconnect/reconnect cleanly
- Surface receiver state to the UI/app state layer
- Add logs and metrics for:
  - packets/frames received
  - buffer fill
  - playback state
  - underruns/overruns
  - reconnect attempts

Important:
- Keep playback engine modular
- Avoid hardcoding everything into one file
- Make the receiver transport contract clear and documented

Deliverables:
- receiver daemon/service module
- playback engine
- state model
- docs/receiver-mode.md

Validation:
- app compiles
- receiver mode can be started/stopped from internal app flow
- summary explains how incoming frames are handed to playback
```
