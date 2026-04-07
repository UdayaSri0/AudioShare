# Prompt 0 — master operating instruction for Codex

```text
You are working inside an existing repository for a new open-source Linux desktop application.

Project goal:
Build a modern Linux audio streaming and casting application that can capture system audio/output audio from the Linux machine and stream it to other devices while optionally continuing playback on the local machine at the same time.

Primary product vision:
- Linux-first desktop GUI application
- Sender machine can capture desktop/system audio
- Sender can continue local playback while also casting
- Stream over Wi‑Fi/LAN first
- Bluetooth support should be designed as a later phase, not hacked in early
- Architecture must be modular so future Windows support is possible
- Clean, production-grade codebase
- Strong separation between UI, audio engine, transport, discovery, and device management
- Open-source friendly structure and documentation

Preferred stack:
- Rust for the core application and performance-sensitive modules
- GTK4 + libadwaita for modern Linux GUI
- PipeWire integration for Linux audio capture and playback
- GStreamer only if truly needed and justified
- Serde for config/data models
- Tokio or equivalent async runtime for networking
- mDNS/zeroconf discovery for LAN device discovery
- A portable core architecture so the networking/streaming layer is not tightly coupled to Linux UI code

Non-negotiable engineering rules:
- Do not rewrite working parts unnecessarily
- Do not produce placeholder architecture with fake implementations
- Do not remove features unless clearly justified in comments and docs
- Keep the code idiomatic, strongly typed, and production-oriented
- Add logging, error handling, and sensible defaults
- Add README updates whenever a major feature is added
- Add tests where practical
- Add clear TODO markers only for genuinely deferred items
- Prefer small, reviewable commits
- If a design choice is made, document it in docs/adr or docs/architecture

Expected UX:
- Modern GUI with a polished Linux-native feel
- Device discovery view
- Audio source/output selection
- Cast start/stop controls
- Local playback mirror toggle
- Status indicators for connected devices
- Latency/quality settings
- Logs/diagnostics page
- Settings page
- About page

Work style:
For every task:
1. inspect the repo first
2. explain the current state briefly
3. propose the exact files to create/update
4. implement the task
5. run tests/build/format/lint if available
6. summarize what changed
7. list any follow-up risks or next steps

Never pretend code was tested if it was not.
Never claim a feature is complete unless it is integrated end-to-end.
```
