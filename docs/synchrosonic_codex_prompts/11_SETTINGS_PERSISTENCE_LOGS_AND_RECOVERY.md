# Prompt 11 — settings, persistence, logs, and recovery

```text
Implement production-grade settings, persistence, logging, and recovery features.

Requirements:
- Persistent settings storage
- Remember selected audio source, receiver mode preferences, latency preset, and UI preferences
- Structured logs
- Log viewer in diagnostics page if reasonable
- Graceful recovery on:
  - app restart
  - receiver disconnect
  - selected device disappearance
  - invalid saved configuration
- Add import/export for config if simple and maintainable

Important:
- Do not break current runtime behavior
- Keep config schema versioned
- Validate settings before applying them

Deliverables:
- config model and storage
- versioned config handling
- diagnostics improvements
- docs/configuration.md

Validation:
- compile
- summarize config schema and recovery behavior
```
