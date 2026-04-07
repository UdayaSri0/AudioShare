# Prompt 10 — build the modern GTK4/libadwaita GUI

```text
Build the production-ready GTK4/libadwaita GUI for the application.

Goal:
Create a modern Linux-native interface for managing capture, discovery, casting, diagnostics, and settings.

Screens/views wanted:
- Dashboard / home
- Discovered devices
- Active casting sessions
- Audio source and output selection
- Receiver mode panel
- Diagnostics/logs
- Settings
- About

UI requirements:
- modern libadwaita design
- responsive layout
- clear state-driven updates
- useful empty states and error messages
- polished labels and user-facing wording
- avoid clutter
- accessible controls where practical

Functional controls:
- choose source
- choose one or more target devices
- start/stop cast
- enable/disable local mirror
- enter receiver mode
- quality/latency preset selection
- view device/session health
- diagnostics panel

Important:
- UI must bind to app state cleanly
- do not bury logic inside widgets
- keep code modular and readable

Deliverables:
- polished GUI
- state wiring
- About page with project/developer/open-source info placeholders
- updated README screenshots section placeholder if real screenshots are not yet available

Validation:
- compile and run
- summarize all main screens and their connected behaviors
```
