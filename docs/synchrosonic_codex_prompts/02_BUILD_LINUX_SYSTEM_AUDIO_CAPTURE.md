# Prompt 2 — build Linux system audio capture

```text
Implement the Linux audio capture layer for the app.

Goal:
Capture system/output audio from the Linux machine in a robust way suitable for streaming and optional local monitoring.

Requirements:
- Use PipeWire-first architecture
- Enumerate available audio sinks / capture-capable sources
- Allow selecting the current source from the app state layer
- Build the capture pipeline cleanly so it can feed:
  1. local monitoring/playback
  2. network streaming encoder
- Expose audio frames through a well-defined internal interface
- Handle source changes, disconnects, and device errors gracefully
- Add structured logging for audio initialization, source selection, stream state, and errors

Important constraints:
- Do not couple PipeWire-specific code directly to UI widgets
- Keep the capture interface portable so future non-Linux backends can be added
- Keep buffering and latency handling explicit in the code

What to add:
- domain models for audio devices and capture settings
- Linux PipeWire backend
- tests or integration checks where realistic
- docs explaining how the capture path works

Validation:
- project must still compile
- include one simple developer-visible way to confirm capture is active, such as logs, debug stats, or a waveform/level indicator hook
- summarize how the capture frames are exposed to the next layer
```

used