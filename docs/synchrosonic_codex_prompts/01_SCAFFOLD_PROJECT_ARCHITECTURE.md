# Prompt 1 — scaffold the full project architecture

```text
Using the repository currently open in VS Code, create the initial production-ready project architecture for the Linux audio streaming app.

What I want in this phase:
- Inspect the repo first and preserve anything useful already present
- Create a clean modular architecture for:
  - desktop GUI
  - core domain models
  - audio capture/playback
  - discovery
  - network transport
  - receiver mode
  - configuration
  - logging/diagnostics
- Prefer a Rust workspace if that is the best fit
- Separate Linux-specific implementation from portable core logic
- Add docs/architecture overview
- Add docs/roadmap.md
- Add docs/adr/ with at least one ADR explaining the chosen stack and modular design
- Add a clear README with goals, non-goals, and development setup

Important:
- Do not implement heavy feature logic yet
- Focus on clean structure, app startup flow, state management boundaries, and module responsibilities
- Create compileable skeleton code, not pseudocode
- Add placeholders only where justified, with comments

Deliverables:
- project structure
- compileable baseline app
- architecture documentation
- TODO roadmap for next phases

At the end:
- build the project
- fix compile errors
- summarize every created file and why it exists
```
used 