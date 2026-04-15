# SynchroSonic — Complete AI Agent Build Brief (v2 / Linux-first)
> Historical/deprecated planning brief: this file describes an earlier
> Python/PySide6 + Snapcast design, not the current Rust/GTK application in this
> repository.
> Copy this entire document into your coding agent (Cursor, Windsurf, Claude Code, etc.)
> and instruct it to execute each phase in order, with zero skipping.

---

## 0) AGENT OPERATING RULES (NON‑NEGOTIABLE)

1. **Execute phases in strict order.** No jumping ahead.
2. **No placeholders in production paths.** No `TODO`, no `pass`, no stub methods. If something is not in scope, remove it or move it to “Future” docs.
3. **Every shell script must use:** `set -euo pipefail` and print actionable error messages.
4. **No hardcoded paths** outside `synchrosonic/core/constants.py`.
5. **Every module gets tests** (unit tests or explicitly documented manual verification).
6. **Commit after each phase** with message format: `[phase-N] short description`.
7. Prefer `pathlib.Path`, `subprocess` (never `os.system`), and `asyncio` for I/O.
8. Don’t add dependencies without adding to `pyproject.toml` + `requirements.txt` and justifying in README.

---

## 1) PROJECT IDENTITY (LOCKED DECISIONS)

| Field | Value |
|---|---|
| App name | SynchroSonic |
| Purpose | Stream **Linux system audio output** in tight sync to multiple LAN devices |
| License | GPLv3 |
| Primary language | Python 3.11+ |
| GUI framework | PySide6 (Qt6 for Python) + qasync |
| Sync engine | Snapcast (snapserver + snapclient) |
| Audio capture | PipeWire capture via **pw-cat/pw-record with `--raw`** (fallback: `parec`) |
| Snapcast source type | `pipe:///<fifo>?name=SystemAudio...` |
| Snapcast codec | `pcm` for MVP (no codec latency; higher bandwidth) |
| Config | TOML (`tomllib` read + `tomli-w` write) |
| Control API | Snapserver JSON-RPC over TCP (1705) |
| Service mgmt | systemd **user** services for MVP (no root required) |
| Targets | MVP: Linux server + Linux GUI controller. Later: Windows/macOS GUI + mobile client guidance |

### 1.1 What “Bluetooth + Wi‑Fi together” means in this design
- Wi‑Fi/LAN carries the synchronized stream to each receiver device.
- A Bluetooth speaker is **only a final output** for a receiver device (phone/laptop). The speaker itself cannot run SynchroSonic.

---

## 2) CRITICAL CORRECTNESS REQUIREMENTS (READ THIS TWICE)

### 2.1 Raw PCM requirement (do not break this)
Snapserver `pipe://` expects a fixed sampleformat stream (e.g., `48000:16:2`).  
PipeWire tools (`pw-cat` / `pw-record`) will **guess a container** (WAV by default) if you write to a filename without forcing raw. Therefore:

✅ MUST: capture using `--raw` and write to FIFO **via stdout redirection**  
Example concept:
- `pw-record --raw ... - > FIFO`

### 2.2 Snapcast source URI must follow official format
Use:
- `pipe:///<path/to/pipe>?name=<name>[&mode=create|read]`
- global/per-source `sampleformat`, `codec`, `buffer` etc.

### 2.3 “Done” means reproducible
At the end of Phase 6:
- GUI launches
- “Start Casting” starts snapserver + capture
- A second Linux device running snapclient plays in sync
- Device list loads via JSON-RPC
- Volume + delay controls work

---

## 3) COMPLETE REPOSITORY STRUCTURE (UPDATED)

Create this exact tree:

```
synchrosonic/
├── .github/workflows/ci.yml
├── configs/
│   ├── snapserver.conf.template
│   └── systemd/
│       ├── synchrosonic-capture.service.template
│       └── synchrosonic-snapserver.service.template
├── daemon/
│   ├── __init__.py
│   ├── rpc_client.py
│   ├── process_manager.py
│   ├── pipewire.py
│   └── fifo.py                  # FIFO helpers (is_fifo, ensure_fifo)
├── docs/
│   ├── architecture.md
│   ├── security.md
│   └── troubleshooting.md
├── gui/
│   ├── __init__.py
│   ├── app.py
│   ├── main_window.py
│   ├── pages/
│   │   ├── __init__.py
│   │   ├── dashboard.py
│   │   ├── devices.py
│   │   ├── groups.py
│   │   └── calibration.py
│   ├── widgets/
│   │   ├── __init__.py
│   │   ├── client_card.py
│   │   ├── log_panel.py
│   │   └── source_selector.py
│   └── assets/
│       ├── __init__.py          # REQUIRED for importlib.resources
│       ├── style.qss
│       └── icons/
├── scripts/
│   ├── preflight.sh
│   ├── install_deps_ubuntu.sh
│   ├── setup_server.sh
│   └── setup_client.sh
├── synchrosonic/
│   ├── __init__.py
│   ├── core/
│   │   ├── __init__.py
│   │   ├── constants.py
│   │   ├── config.py
│   │   └── logger.py
│   └── models/
│       ├── __init__.py
│       ├── client.py
│       └── group.py
├── tests/
│   ├── __init__.py
│   ├── test_config.py
│   ├── test_fifo.py
│   ├── test_pipewire.py
│   ├── test_rpc_client.py
│   └── test_process_manager.py
├── packaging/
│   └── deb/
│       ├── control
│       ├── postinst
│       └── postrm
├── .gitignore
├── LICENSE
├── README.md
├── pyproject.toml
└── requirements.txt
```

---

## 4) PHASE 0 — PREFLIGHT (FAIL FAST)

### 4.1 Implement `scripts/preflight.sh` (REQUIRED)
Must:
- Check `python3 --version` >= 3.11
- Check commands exist: `snapserver`, `snapclient`, `pw-record` OR `pw-cat`, `pactl`
- Check PipeWire is running: `systemctl --user is-active pipewire`
- Print clear next actions if something missing
- Exit non-zero on failure

Manual verification:
- Run: `scripts/preflight.sh`
- Must pass before Phase 1 continues

---

## 5) PHASE 1 — SCAFFOLDING & CONFIG

### 5.1 `pyproject.toml`
Requirements:
- Ensure packages include `synchrosonic`, `gui`, `daemon` (setuptools find should discover them).
- Keep entrypoint: `synchrosonic = "gui.app:main"`

### 5.2 Constants
Keep your existing constant list, but add:
- `PW_CAT_BIN = "pw-cat"`
- `CAPTURE_METHOD = "auto"` (auto|pwcat|parec)
- `SNAPSERVER_HTTP_PORT = 1780`

---

## 6) PHASE 2 — FIFO HELPERS (PYTHON 3.11 SAFE)

Create `daemon/fifo.py`

Requirements:
- Implement `is_fifo(path: Path) -> bool` using `stat.S_ISFIFO(path.stat().st_mode)`
- Implement `ensure_fifo(path: Path) -> None`:
  - create parent dirs
  - if exists and not FIFO: delete and recreate
  - create FIFO with `os.mkfifo`

Add `tests/test_fifo.py`.

---

## 7) PHASE 3 — PIPEWIRE SOURCE DISCOVERY

Update `daemon/pipewire.py`:

Must:
- List monitor sources via `pw-cli` and fall back to `pactl`
- Mark default monitor:
  - `pactl get-default-sink` => `<sink>.monitor`

Nice-to-have:
- Add `wpctl status` parsing fallback if both fail

---

## 8) PHASE 4 — SNAPSERVER CONFIG TEMPLATE (FIXED)

Replace the snapserver template with a **single** `[stream]` section and correct `pipe://` source format.

Create `configs/snapserver.conf.template`:

Placeholders:
- `{{FIFO_PATH}}`
- `{{DATA_DIR}}`
- `{{BUFFER_MS}}`

Example structure (final implementation must be valid Snapcast config):

```
[server]
datadir = {{DATA_DIR}}

[stream]
buffer = {{BUFFER_MS}}
codec = pcm
sampleformat = 48000:16:2

source = pipe:///{{FIFO_PATH}}?name=SystemAudio&mode=read
```

---

## 9) PHASE 5 — CAPTURE IMPLEMENTATION (MOST IMPORTANT)

### 9.1 Update `daemon/process_manager.py` capture strategy

Implement capture in this order:
1) Prefer `pw-record` or `pw-cat` **with `--raw`**, `--target`, `--rate`, `--channels`, `--format`
2) Output to stdout (`-`) and redirect to FIFO using a shell wrapper, e.g.:

```
/bin/bash -lc 'pw-record --raw --target "<monitor>" --rate 48000 --channels 2 --format s16 - > "<fifo>"'
```

Fallback:
- `parec -d "<monitor>" --format=s16le --rate=48000 --channels=2 > "<fifo>"`

### 9.2 Add a PCM sanity check
On start, implement a quick validation:
- Detect if the stream begins with `RIFF` (WAV header)
- If detected: warn **raw PCM required** and fail fast

Add `tests/test_process_manager.py` (mock subprocess calls).

---

## 10) PHASE 6 — RPC CLIENT HARDENING

Update `SnapRPCClient`:
- reconnect on connection loss with exponential backoff
- cleanup pending futures on timeout
- never leave `_pending` entries dangling
- add `async with SnapRPCClient():` support

Update tests accordingly.

---

## 11) PHASE 7 — GUI (MODERN, CLEAN, NO CRASHES)

### 11.1 UI requirements
- Left nav rail
- Pages: Dashboard, Devices, Groups, Calibration
- Dark theme (QSS ok)
- Empty states + actionable errors

### 11.2 Dashboard behaviour
- Source dropdown: loads monitor sources + marks default
- “Start Casting”:
  - Generates snapserver.conf from template
  - Starts snapserver
  - Starts capture (raw PCM)
  - Shows status
- “Stop”:
  - stops capture
  - stops snapserver
- On app exit: always stop both

### 11.3 Devices page
- Poll every 4s:
  - connect to RPC if needed
  - show cards
  - allow volume/mute/delay
- Debounce RPC calls:
  - volume + delay changes send after 150ms pause

### 11.4 Calibration page
- Test tone optional
- Must not crash if `sox` missing (show friendly error)

---

## 12) PHASE 8 — SCRIPTS (UPDATED)

### 12.1 `install_deps_ubuntu.sh`
Add:
- `pipewire-bin` (tool availability can differ by distro packaging)
- `pulseaudio-utils` (`pactl`, `parec`)
End:
- run `scripts/preflight.sh` and print next steps

### 12.2 `setup_server.sh`
Must:
- generate config with buffer placeholder
- generate systemd **user** services:
  - capture uses raw PCM stdout → FIFO
  - snapserver uses generated config
Must NOT:
- auto-start services (GUI controls this)

### 12.3 `setup_client.sh`
- keep snapclient systemd user service
- print Bluetooth output note

---

## 13) PHASE 9 — DOCS (MVP QUALITY)

### `docs/architecture.md` MUST include:
- PipeWire monitor capture → FIFO → snapserver pipe source → LAN → snapclient
- Why Bluetooth speakers can’t run software; receivers output to BT locally
- How sync works at a high level (buffering, latency)

### `docs/troubleshooting.md` MUST include:
- “No monitor sources found”
- “WAV header detected / no sound”
- “Clients out of sync: increase buffer”
- “BT speaker silent: set BT sink default on receiver”

### `docs/security.md` MUST include:
- MVP is **trusted LAN only**
- Recommend firewalling snapserver ports to LAN
- Future: auth/TLS guidance (optional)

---

## 14) PHASE 10 — CI

CI requirements:
- headless Qt tests via Xvfb
- ruff checks
- no network needed for test stage

---

## 15) PHASE 11 — PACKAGING (IMPORTANT FIX)

### Rule:
**Do NOT use `pip --break-system-packages` in postinst.**

MVP packaging choice (pick one):
A) Skip `.deb` for v0.1.0 and ship source + scripts (recommended for first public tag)
B) Proper `.deb`:
   - venv under `/opt/synchrosonic/venv`
   - build-time vendoring; postinst only wires desktop entry/services

Document the choice in README.

---

## 16) EXECUTION ORDER CHECKLIST

- [ ] Phase 0: preflight.sh passes
- [ ] Phase 1: core scaffolding imports cleanly
- [ ] Phase 2: FIFO helpers work + tests pass
- [ ] Phase 3: monitor source discovery works
- [ ] Phase 4: snapserver.conf generated and valid
- [ ] Phase 5: capture produces raw PCM (no WAV header)
- [ ] Phase 6: RPC client stable + reconnect works
- [ ] Phase 7: GUI launches and controls casting
- [ ] Phase 8: scripts work on Ubuntu 22.04/24.04
- [ ] Phase 9: docs complete
- [ ] Phase 10: CI green
- [ ] Phase 11: packaging decision documented

---

## 17) MVP ACCEPTANCE TEST (“USER STORY”)

1) On Linux Server:
- Run `scripts/install_deps_ubuntu.sh`
- Run `scripts/setup_server.sh`
- Launch GUI: `synchrosonic`
- Select monitor source
- Click **Start Casting**

2) On Receiver Linux:
- Run `scripts/setup_client.sh <server-ip>`
- Audio plays in sync

3) Optional Bluetooth output:
- Pair Bluetooth speaker to receiver
- Set it as default output on receiver
- Audio continues through Bluetooth (may need delay tuning)

---

*End of build brief (v2).*
