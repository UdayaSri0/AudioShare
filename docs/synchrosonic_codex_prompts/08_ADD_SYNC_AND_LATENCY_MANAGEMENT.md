# Prompt 8 — add basic sync and latency management

```text
Implement first-pass synchronization and latency management for multi-device playback.

Goal:
Multiple receivers should play close enough together for normal home/office usage.

Requirements:
- Add timestamping or clock reference design
- Add receiver buffering strategy that can target a requested latency window
- Add a simple sync approach first; do not over-engineer a perfect distributed clock system in v1
- Provide configurable latency presets such as:
  - low latency
  - balanced
  - stable
- Expose buffer/sync health in diagnostics

Important:
- Document assumptions and limitations honestly
- Keep implementation understandable and debuggable
- Make the timing code explicit and well-commented

Deliverables:
- sync timing logic
- latency presets
- diagnostics output
- docs/synchronization.md with limitations and future improvements

Validation:
- compile
- explain how timestamps/latency are managed in the current version
```
