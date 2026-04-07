# Prompt 7 — add multi-device casting

```text
Implement multi-device casting support.

Goal:
A sender can stream to multiple discovered receiver devices at once.

Requirements:
- Support multiple active receiver sessions
- Each device should have:
  - connection state
  - health status
  - last error
  - latency/buffer indicators if possible
- Sender should be able to start/stop individual targets without collapsing the whole session manager
- Build the fan-out architecture so one capture stream can serve multiple receiver sessions
- If one receiver fails, the others should continue where possible
- Add clear logs and UI state for per-device session health

Important:
- Keep the design scalable and modular
- Do not entangle per-device state with global app state in a messy way
- Add internal abstractions for session collection / target manager

Deliverables:
- multi-device sender session manager
- per-device target state
- UI-facing models for active casts
- docs/multi-device-streaming.md

Validation:
- compile
- summarize the concurrency model and failure isolation strategy
```
