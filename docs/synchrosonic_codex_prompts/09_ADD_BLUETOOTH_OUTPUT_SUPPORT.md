# Prompt 9 — add Bluetooth output support as a controlled phase

```text
Implement Bluetooth support carefully as a separate phase.

Goal:
Allow the app to route playback to Bluetooth audio outputs where practical, without destabilizing the main Wi‑Fi/LAN streaming architecture.

Requirements:
- Do not redesign the whole app
- Keep Bluetooth support behind a clean abstraction
- Add detection/enumeration of Bluetooth-capable playback outputs on Linux
- Allow a receiver or local playback path to target a Bluetooth output device when selected
- Add clear capability/state reporting in UI/app state
- Handle Bluetooth device disconnect/reconnect events gracefully

Important constraints:
- Keep Wi‑Fi/LAN streaming as the main architecture
- Bluetooth should integrate as an output target, not as a hack across all modules
- If any limitations exist, document them clearly rather than hiding them

Deliverables:
- Bluetooth output abstraction
- Linux implementation
- settings/state integration
- docs/bluetooth-support.md with limitations and usage notes

Validation:
- compile
- summarize exactly what Bluetooth support means in this version
```

used
v0.0.9