# Prompt 6 — add simultaneous local playback while casting

```text
Implement simultaneous local playback while casting.

Goal:
The sender machine must be able to continue playing audio locally while also streaming the same audio to remote receiver devices.

Requirements:
- Add a local playback mirror toggle
- Build the audio pipeline so one captured stream can fan out to:
  1. network sender
  2. local playback path
- Ensure the pipeline is stable and does not block or degrade badly when one branch is slower
- Add sensible buffering strategies
- Handle enabling/disabling local playback during an active cast session
- Expose status and errors clearly in app state and logs

Important:
- Do not duplicate fragile logic
- Keep fan-out routing explicit and maintainable
- Make it easy to extend later to multiple remote receivers

Deliverables:
- local mirror pipeline
- app state toggle
- diagnostics for mirror on/off and any stream branch issues
- updated docs explaining the split-stream architecture

Validation:
- local playback and remote playback can coexist
- compile and summarize the branch/fan-out design clearly
```
