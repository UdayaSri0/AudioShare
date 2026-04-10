# Contributing To SynchroSonic

Thanks for helping build SynchroSonic.

## Ground Rules

- Keep Linux Wi-Fi/LAN streaming as the main architecture.
- Prefer understandable, debuggable implementations over clever hidden behavior.
- Document real limitations instead of smoothing them over.
- Avoid broad refactors unless they are needed for the task at hand.

## Development Setup

Install Rust plus the Linux build dependencies used by the GTK application:

```bash
sudo apt install build-essential pkg-config libgtk-4-dev libadwaita-1-dev
```

The current audio backend also expects PipeWire command-line tools at runtime:

- `pw-dump`
- `pw-record`
- `pw-play`

## Useful Commands

```bash
cargo fmt --all
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p synchrosonic-app
bash scripts/package-linux.sh
```

## Before Opening A Pull Request

- Keep changes focused.
- Add or update docs when behavior changes.
- Add tests when a change affects validation, parsing, state transitions, or recovery.
- Run the formatting, lint, and test commands above.
- If you touched packaging assets, also run `bash scripts/package-linux.sh --skip-build`.

## Pull Request Expectations

- Explain the user-visible behavior change.
- Call out tradeoffs or limitations honestly.
- Mention packaging or runtime dependency changes explicitly.
- Include screenshots only when UI changes are intentional and stable enough to review.

## Reporting Bugs

Please use the issue templates so reports include:

- Linux distribution and version
- desktop environment or compositor when relevant
- PipeWire version or relevant audio tooling details
- exact reproduction steps
- diagnostic/log output when available

Security-sensitive problems should follow [SECURITY.md](/home/strix/Documents/GitHub/AudioShare/SECURITY.md) instead of being filed publicly.

## Documentation And Release Work

Release-oriented changes should stay aligned with:

- [docs/linux-packaging.md](/home/strix/Documents/GitHub/AudioShare/docs/linux-packaging.md)
- [docs/release-checklist.md](/home/strix/Documents/GitHub/AudioShare/docs/release-checklist.md)
- [docs/configuration.md](/home/strix/Documents/GitHub/AudioShare/docs/configuration.md)

If you add a new dependency, packaging step, or support promise, update the relevant docs in the same change.
