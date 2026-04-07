# SynchroSonic — Complete AI Agent Build Brief
> Copy this entire document into your coding agent (Cursor, Windsurf, Claude Code, etc.)
> and instruct it to execute each phase in order without skipping steps.

---

## 0. AGENT OPERATING RULES

Before writing a single line of code, internalize these rules. Violating them will cause build failures.

1. **Execute phases in strict order.** Never jump ahead. Each phase outputs files that the next phase depends on.
2. **Never use placeholder comments** like `# TODO`, `# implement later`, or `pass` in production paths. Every function must be fully implemented.
3. **Every shell command must handle errors.** Use `set -euo pipefail` in all bash scripts. Wrap subprocess calls in try/except with logged output.
4. **No hardcoded paths except in constants files.** All configurable paths live in `synchrosonic/core/constants.py`.
5. **Test every file you create.** After writing a module, write its corresponding test in `tests/`. If a test cannot be automated, document a manual verification step.
6. **Commit message format:** `[phase-N] short description` after each phase.
7. **When in doubt about a Linux API**, prefer `subprocess` over `os.system`, prefer `pathlib.Path` over string paths, and prefer `asyncio` over threading for I/O.
8. **Do not install packages not listed here** without adding them to `requirements.txt` and explaining why.

---

## 1. PROJECT IDENTITY

| Field | Value |
|---|---|
| App name | SynchroSonic |
| Purpose | Stream Linux system audio in tight sync to multiple LAN devices |
| License | GPLv3 |
| Primary language | Python 3.11+ |
| GUI framework | PySide6 (Qt6 for Python) — chosen for native Linux integration, mature async support via `qasync`, and direct D-Bus/PipeWire interop without a Wayland bridge |
| Sync engine | Snapcast (snapserver + snapclient) |
| Audio capture | PipeWire via `pw-record` or `parecord` (PulseAudio compat layer) |
| Config management | TOML via `tomllib` (stdlib 3.11) + `tomli-w` for writing |
| IPC with snapserver | JSON-RPC 2.0 over TCP (port 1705) |
| Service management | systemd user services (no root required for MVP) |
| Package targets | `.deb` (Ubuntu 22.04+) and Flatpak (future) |

---

## 2. COMPLETE REPOSITORY STRUCTURE

Create this exact tree. Do not rename directories.

```
synchrosonic/
├── .github/
│   └── workflows/
│       └── ci.yml
├── configs/
│   ├── snapserver.conf.template
│   └── systemd/
│       ├── synchrosonic-capture.service.template
│       └── synchrosonic-snapserver.service.template
├── daemon/
│   ├── __init__.py
│   ├── rpc_client.py          # JSON-RPC 2.0 client for snapserver :1705
│   ├── process_manager.py     # Start/stop/restart capture + snapserver
│   └── pipewire.py            # Query PipeWire monitor sources via pw-cli
├── docs/
│   ├── architecture.md
│   ├── security.md
│   └── troubleshooting.md
├── gui/
│   ├── __init__.py
│   ├── app.py                 # QApplication entry point
│   ├── main_window.py         # MainWindow + page router
│   ├── pages/
│   │   ├── __init__.py
│   │   ├── dashboard.py       # "Start Casting" + status
│   │   ├── devices.py         # Client list, volume, delay, mute
│   │   ├── groups.py          # Group management
│   │   └── calibration.py     # Test tone + latency adjustment
│   ├── widgets/
│   │   ├── __init__.py
│   │   ├── client_card.py     # Single device card widget
│   │   ├── log_panel.py       # Scrollable log viewer
│   │   └── source_selector.py # PipeWire monitor source dropdown
│   └── assets/
│       ├── style.qss          # Qt stylesheet
│       └── icons/             # SVG icons (placeholder PNGs acceptable for MVP)
├── scripts/
│   ├── install_deps_ubuntu.sh
│   ├── setup_server.sh
│   └── setup_client.sh
├── synchrosonic/
│   ├── __init__.py
│   ├── core/
│   │   ├── __init__.py
│   │   ├── constants.py       # All paths, ports, defaults
│   │   ├── config.py          # Read/write TOML config
│   │   └── logger.py          # Structured logging setup
│   └── models/
│       ├── __init__.py
│       ├── client.py          # SnapClient dataclass
│       └── group.py           # SnapGroup dataclass
├── tests/
│   ├── __init__.py
│   ├── test_rpc_client.py
│   ├── test_process_manager.py
│   ├── test_config.py
│   └── test_pipewire.py
├── packaging/
│   └── deb/
│       ├── control
│       ├── postinst
│       └── postrm
├── .gitignore
├── LICENSE                    # Full GPLv3 text
├── README.md
├── pyproject.toml
└── requirements.txt
```

---

## 3. PHASE 1 — SCAFFOLDING & CONFIGURATION

### 3.1 Create `pyproject.toml`

```toml
[build-system]
requires = ["setuptools>=68", "wheel"]
build-backend = "setuptools.backends.legacy:build"

[project]
name = "synchrosonic"
version = "0.1.0"
description = "Synchronized multi-room audio streaming for Linux"
license = { text = "GPL-3.0-or-later" }
requires-python = ">=3.11"
dependencies = [
    "PySide6>=6.6.0",
    "qasync>=0.27.0",
    "tomli-w>=1.0.0",
    "zeroconf>=0.131.0",
    "aiohttp>=3.9.0",
]

[project.scripts]
synchrosonic = "gui.app:main"

[tool.setuptools.packages.find]
where = ["."]
```

### 3.2 Create `requirements.txt`

```
PySide6>=6.6.0
qasync>=0.27.0
tomli-w>=1.0.0
zeroconf>=0.131.0
aiohttp>=3.9.0
pytest>=8.0.0
pytest-asyncio>=0.23.0
pytest-qt>=4.3.1
```

### 3.3 Create `synchrosonic/core/constants.py`

Implement every constant below. Do not use string literals elsewhere in the codebase for these values.

```python
from pathlib import Path
import os

# Directories
XDG_CONFIG = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config"))
XDG_DATA = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local/share"))
XDG_STATE = Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state"))

APP_NAME = "synchrosonic"
CONFIG_DIR = XDG_CONFIG / APP_NAME
DATA_DIR = XDG_DATA / APP_NAME
STATE_DIR = XDG_STATE / APP_NAME
LOG_DIR = STATE_DIR / "logs"
SYSTEMD_USER_DIR = Path.home() / ".config/systemd/user"

# Files
CONFIG_FILE = CONFIG_DIR / "config.toml"
FIFO_PATH = DATA_DIR / "system-audio.fifo"
SNAPSERVER_CONF = CONFIG_DIR / "snapserver.conf"
LOG_FILE = LOG_DIR / "synchrosonic.log"

# Snapcast
SNAPSERVER_PORT_STREAM = 1704       # Audio stream port
SNAPSERVER_PORT_CONTROL = 1705      # JSON-RPC control port
SNAPSERVER_HOST = "127.0.0.1"
SNAPSERVER_BIN = "snapserver"
SNAPCLIENT_BIN = "snapclient"

# PipeWire
PW_RECORD_BIN = "pw-record"
PARECORD_BIN = "parecord"           # Fallback via PulseAudio compat

# Audio format (must match snapserver stream config)
AUDIO_SAMPLE_RATE = 48000
AUDIO_CHANNELS = 2
AUDIO_BIT_DEPTH = 16

# Defaults
DEFAULT_BUFFER_MS = 1000
DEFAULT_LATENCY_MS = 0
MAX_LATENCY_MS = 2000
TEST_TONE_FREQ_HZ = 1000
TEST_TONE_DURATION_S = 3
```

### 3.4 Create `synchrosonic/core/config.py`

Full implementation — reads and writes TOML. Creates default config if missing.

```python
import tomllib
import tomli_w
from pathlib import Path
from synchrosonic.core.constants import CONFIG_FILE, CONFIG_DIR, FIFO_PATH, SNAPSERVER_CONF
from synchrosonic.core.logger import get_logger

logger = get_logger(__name__)

DEFAULT_CONFIG = {
    "server": {
        "fifo_path": str(FIFO_PATH),
        "snapserver_conf": str(SNAPSERVER_CONF),
        "buffer_ms": 1000,
    },
    "audio": {
        "monitor_source": "",   # populated after user selects source
    },
    "ui": {
        "theme": "system",      # "light" | "dark" | "system"
    },
    "security": {
        "token": "",            # optional shared token; empty = disabled
    },
}


def load_config() -> dict:
    """Load config from disk. Creates default config if missing."""
    CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    if not CONFIG_FILE.exists():
        save_config(DEFAULT_CONFIG)
        logger.info("Created default config at %s", CONFIG_FILE)
    with open(CONFIG_FILE, "rb") as f:
        data = tomllib.load(f)
    # Merge with defaults to handle missing keys from older versions
    return _deep_merge(DEFAULT_CONFIG, data)


def save_config(config: dict) -> None:
    """Persist config to disk atomically."""
    CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    tmp = CONFIG_FILE.with_suffix(".toml.tmp")
    with open(tmp, "wb") as f:
        tomli_w.dump(config, f)
    tmp.replace(CONFIG_FILE)
    logger.debug("Config saved to %s", CONFIG_FILE)


def _deep_merge(base: dict, override: dict) -> dict:
    result = base.copy()
    for k, v in override.items():
        if k in result and isinstance(result[k], dict) and isinstance(v, dict):
            result[k] = _deep_merge(result[k], v)
        else:
            result[k] = v
    return result
```

### 3.5 Create `synchrosonic/core/logger.py`

```python
import logging
import sys
from pathlib import Path
from synchrosonic.core.constants import LOG_DIR, LOG_FILE

_configured = False


def setup_logging(level: int = logging.DEBUG) -> None:
    global _configured
    if _configured:
        return
    LOG_DIR.mkdir(parents=True, exist_ok=True)

    fmt = logging.Formatter(
        "%(asctime)s [%(levelname)s] %(name)s: %(message)s",
        datefmt="%Y-%m-%dT%H:%M:%S",
    )
    root = logging.getLogger()
    root.setLevel(level)

    # Console handler
    ch = logging.StreamHandler(sys.stdout)
    ch.setFormatter(fmt)
    root.addHandler(ch)

    # Rotating file handler
    from logging.handlers import RotatingFileHandler
    fh = RotatingFileHandler(LOG_FILE, maxBytes=5 * 1024 * 1024, backupCount=3)
    fh.setFormatter(fmt)
    root.addHandler(fh)

    _configured = True


def get_logger(name: str) -> logging.Logger:
    setup_logging()
    return logging.getLogger(name)
```

---

## 4. PHASE 2 — DATA MODELS

### 4.1 Create `synchrosonic/models/client.py`

```python
from dataclasses import dataclass, field
from typing import Optional


@dataclass
class SnapClient:
    id: str
    name: str
    ip: str
    connected: bool
    volume: int          # 0–100
    muted: bool
    latency_ms: int      # per-client delay in ms
    group_id: Optional[str] = None
    output_note: str = "Internal"  # user-managed label shown in UI

    @classmethod
    def from_rpc(cls, data: dict) -> "SnapClient":
        """Parse from snapserver JSON-RPC client object."""
        cfg = data.get("config", {})
        vol = data.get("volume", {})
        host = data.get("host", {})
        return cls(
            id=data["id"],
            name=cfg.get("name", host.get("name", "Unknown")),
            ip=host.get("ip", ""),
            connected=data.get("connected", False),
            volume=vol.get("percent", 100),
            muted=vol.get("muted", False),
            latency_ms=cfg.get("latency", 0),
        )
```

### 4.2 Create `synchrosonic/models/group.py`

```python
from dataclasses import dataclass, field
from typing import List


@dataclass
class SnapGroup:
    id: str
    name: str
    client_ids: List[str] = field(default_factory=list)
    muted: bool = False
    volume: int = 100

    @classmethod
    def from_rpc(cls, data: dict) -> "SnapGroup":
        return cls(
            id=data["id"],
            name=data.get("name", data["id"][:8]),
            client_ids=[c["id"] for c in data.get("clients", [])],
            muted=data.get("muted", False),
            volume=data.get("volume", {}).get("percent", 100),
        )
```

---

## 5. PHASE 3 — DAEMON LAYER

### 5.1 Create `daemon/rpc_client.py`

Full async JSON-RPC 2.0 client. Every public method must be fully implemented — no stubs.

```python
"""
Async JSON-RPC 2.0 client for snapserver control API (TCP :1705).
Protocol: newline-delimited JSON.
"""

import asyncio
import json
import itertools
from typing import Any, Optional
from synchrosonic.core.constants import SNAPSERVER_HOST, SNAPSERVER_PORT_CONTROL
from synchrosonic.core.logger import get_logger
from synchrosonic.models.client import SnapClient
from synchrosonic.models.group import SnapGroup

logger = get_logger(__name__)
_id_counter = itertools.count(1)


class RPCError(Exception):
    def __init__(self, code: int, message: str):
        super().__init__(f"RPC error {code}: {message}")
        self.code = code


class SnapRPCClient:
    def __init__(
        self,
        host: str = SNAPSERVER_HOST,
        port: int = SNAPSERVER_PORT_CONTROL,
        timeout: float = 5.0,
    ):
        self._host = host
        self._port = port
        self._timeout = timeout
        self._reader: Optional[asyncio.StreamReader] = None
        self._writer: Optional[asyncio.StreamWriter] = None
        self._pending: dict[int, asyncio.Future] = {}
        self._listener_task: Optional[asyncio.Task] = None

    async def connect(self) -> None:
        self._reader, self._writer = await asyncio.wait_for(
            asyncio.open_connection(self._host, self._port),
            timeout=self._timeout,
        )
        self._listener_task = asyncio.create_task(self._listen())
        logger.info("Connected to snapserver RPC at %s:%d", self._host, self._port)

    async def disconnect(self) -> None:
        if self._listener_task:
            self._listener_task.cancel()
            try:
                await self._listener_task
            except asyncio.CancelledError:
                pass
        if self._writer:
            self._writer.close()
            try:
                await self._writer.wait_closed()
            except Exception:
                pass
        logger.info("Disconnected from snapserver RPC")

    async def _listen(self) -> None:
        try:
            while True:
                line = await self._reader.readline()
                if not line:
                    break
                try:
                    msg = json.loads(line.decode())
                except json.JSONDecodeError as e:
                    logger.warning("Malformed RPC response: %s", e)
                    continue
                msg_id = msg.get("id")
                if msg_id and msg_id in self._pending:
                    fut = self._pending.pop(msg_id)
                    if "error" in msg:
                        err = msg["error"]
                        fut.set_exception(RPCError(err["code"], err["message"]))
                    else:
                        fut.set_result(msg.get("result"))
        except asyncio.CancelledError:
            pass
        except Exception as e:
            logger.error("RPC listener error: %s", e)

    async def call(self, method: str, params: Any = None) -> Any:
        """Send a JSON-RPC request and await its response."""
        if not self._writer:
            raise RuntimeError("Not connected to snapserver")
        req_id = next(_id_counter)
        payload = {"jsonrpc": "2.0", "id": req_id, "method": method}
        if params is not None:
            payload["params"] = params
        fut: asyncio.Future = asyncio.get_event_loop().create_future()
        self._pending[req_id] = fut
        self._writer.write((json.dumps(payload) + "\r\n").encode())
        await self._writer.drain()
        return await asyncio.wait_for(fut, timeout=self._timeout)

    # ── High-level helpers ──────────────────────────────────────────────

    async def get_status(self) -> dict:
        return await self.call("Server.GetStatus")

    async def get_clients(self) -> list[SnapClient]:
        status = await self.get_status()
        clients = []
        for group in status.get("server", {}).get("groups", []):
            for c in group.get("clients", []):
                client = SnapClient.from_rpc(c)
                client.group_id = group["id"]
                clients.append(client)
        return clients

    async def get_groups(self) -> list[SnapGroup]:
        status = await self.get_status()
        return [
            SnapGroup.from_rpc(g)
            for g in status.get("server", {}).get("groups", [])
        ]

    async def set_client_volume(self, client_id: str, volume: int, muted: bool = False) -> None:
        await self.call("Client.SetVolume", {
            "id": client_id,
            "volume": {"percent": max(0, min(100, volume)), "muted": muted},
        })

    async def set_client_latency(self, client_id: str, latency_ms: int) -> None:
        await self.call("Client.SetLatency", {
            "id": client_id,
            "latency": max(0, min(2000, latency_ms)),
        })

    async def set_client_name(self, client_id: str, name: str) -> None:
        await self.call("Client.SetName", {"id": client_id, "name": name})

    async def set_group_clients(self, group_id: str, client_ids: list[str]) -> None:
        await self.call("Group.SetClients", {"id": group_id, "clients": client_ids})

    async def set_group_volume(self, group_id: str, volume: int) -> None:
        await self.call("Group.SetVolume", {
            "id": group_id,
            "volume": {"percent": max(0, min(100, volume))},
        })
```

### 5.2 Create `daemon/pipewire.py`

```python
"""
Query PipeWire for available monitor sources using pw-cli.
Falls back to pactl if pw-cli is unavailable.
"""

import asyncio
import re
from dataclasses import dataclass
from synchrosonic.core.logger import get_logger

logger = get_logger(__name__)


@dataclass
class MonitorSource:
    name: str          # e.g. "alsa_output.pci-0000_00_1f.3.analog-stereo.monitor"
    description: str   # human-readable label
    is_default: bool = False


async def list_monitor_sources() -> list[MonitorSource]:
    """Return available PipeWire monitor sources. Never raises — returns [] on failure."""
    sources = await _pw_sources()
    if not sources:
        logger.warning("pw-cli failed or returned no sources; trying pactl fallback")
        sources = await _pactl_sources()
    return sources


async def _run(cmd: list[str]) -> tuple[int, str, str]:
    """Run a subprocess and return (returncode, stdout, stderr)."""
    try:
        proc = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, stderr = await asyncio.wait_for(proc.communicate(), timeout=5.0)
        return proc.returncode, stdout.decode(), stderr.decode()
    except (FileNotFoundError, asyncio.TimeoutError) as e:
        logger.debug("Command %s failed: %s", cmd[0], e)
        return -1, "", str(e)


async def _pw_sources() -> list[MonitorSource]:
    rc, out, _ = await _run(["pw-cli", "list-objects", "Node"])
    if rc != 0:
        return []
    sources = []
    # Parse pw-cli output: look for nodes with media.class = "Audio/Source"
    for block in out.split("id "):
        if "Audio/Source" not in block and "monitor" not in block.lower():
            continue
        name_m = re.search(r'node\.name\s*=\s*"([^"]+)"', block)
        desc_m = re.search(r'node\.description\s*=\s*"([^"]+)"', block)
        if name_m:
            sources.append(MonitorSource(
                name=name_m.group(1),
                description=desc_m.group(1) if desc_m else name_m.group(1),
            ))
    return sources


async def _pactl_sources() -> list[MonitorSource]:
    rc, out, _ = await _run(["pactl", "list", "short", "sources"])
    if rc != 0:
        return []
    sources = []
    for line in out.splitlines():
        parts = line.split()
        if len(parts) >= 2 and "monitor" in parts[1].lower():
            name = parts[1]
            sources.append(MonitorSource(name=name, description=name))
    return sources


async def get_default_monitor() -> str:
    """Return the name of the default audio output monitor source, or empty string."""
    rc, out, _ = await _run(["pactl", "get-default-sink"])
    if rc == 0 and out.strip():
        return out.strip() + ".monitor"
    return ""
```

### 5.3 Create `daemon/process_manager.py`

```python
"""
Manages capture process (pw-record → FIFO) and snapserver lifecycle.
Uses asyncio subprocesses. Integrates with systemd user services when available.
"""

import asyncio
import os
import signal
from pathlib import Path
from typing import Optional
from synchrosonic.core.constants import (
    FIFO_PATH, SNAPSERVER_BIN, PW_RECORD_BIN, PARECORD_BIN,
    AUDIO_SAMPLE_RATE, AUDIO_CHANNELS, AUDIO_BIT_DEPTH,
    SNAPSERVER_CONF,
)
from synchrosonic.core.logger import get_logger

logger = get_logger(__name__)


class ProcessManager:
    def __init__(self):
        self._capture_proc: Optional[asyncio.subprocess.Process] = None
        self._snapserver_proc: Optional[asyncio.subprocess.Process] = None

    # ── FIFO ────────────────────────────────────────────────────────────

    def ensure_fifo(self, path: Path = FIFO_PATH) -> None:
        """Create FIFO pipe if it does not exist."""
        path.parent.mkdir(parents=True, exist_ok=True)
        if path.exists() and not path.is_fifo():
            path.unlink()
        if not path.exists():
            os.mkfifo(path)
            logger.info("Created FIFO at %s", path)

    # ── Capture ─────────────────────────────────────────────────────────

    async def start_capture(self, monitor_source: str, fifo_path: Path = FIFO_PATH) -> None:
        """Start pw-record piping system audio into the FIFO."""
        if await self._capture_running():
            logger.warning("Capture already running; stopping first")
            await self.stop_capture()

        self.ensure_fifo(fifo_path)

        cmd = [
            PW_RECORD_BIN,
            "--target", monitor_source,
            "--channels", str(AUDIO_CHANNELS),
            "--rate", str(AUDIO_SAMPLE_RATE),
            "--format", f"s{AUDIO_BIT_DEPTH}",
            str(fifo_path),
        ]
        logger.info("Starting capture: %s", " ".join(cmd))
        self._capture_proc = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        asyncio.create_task(self._log_stderr(self._capture_proc, "capture"))

    async def stop_capture(self) -> None:
        await self._terminate(self._capture_proc, "capture")
        self._capture_proc = None

    async def _capture_running(self) -> bool:
        return self._capture_proc is not None and self._capture_proc.returncode is None

    # ── Snapserver ──────────────────────────────────────────────────────

    async def start_snapserver(self, conf_path: Path = SNAPSERVER_CONF) -> None:
        if await self._snapserver_running():
            logger.warning("snapserver already running; restarting")
            await self.stop_snapserver()
            await asyncio.sleep(0.5)

        cmd = [SNAPSERVER_BIN, "--config", str(conf_path)]
        logger.info("Starting snapserver: %s", " ".join(cmd))
        self._snapserver_proc = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        asyncio.create_task(self._log_stderr(self._snapserver_proc, "snapserver"))

    async def stop_snapserver(self) -> None:
        await self._terminate(self._snapserver_proc, "snapserver")
        self._snapserver_proc = None

    async def _snapserver_running(self) -> bool:
        return self._snapserver_proc is not None and self._snapserver_proc.returncode is None

    # ── Helpers ─────────────────────────────────────────────────────────

    async def _terminate(
        self, proc: Optional[asyncio.subprocess.Process], name: str
    ) -> None:
        if proc is None or proc.returncode is not None:
            return
        logger.info("Stopping %s (pid %d)", name, proc.pid)
        try:
            proc.send_signal(signal.SIGTERM)
            await asyncio.wait_for(proc.wait(), timeout=5.0)
        except asyncio.TimeoutError:
            logger.warning("%s did not stop gracefully; sending SIGKILL", name)
            proc.kill()
            await proc.wait()

    @staticmethod
    async def _log_stderr(proc: asyncio.subprocess.Process, name: str) -> None:
        if proc.stderr is None:
            return
        async for line in proc.stderr:
            logger.debug("[%s] %s", name, line.decode().rstrip())

    async def status(self) -> dict:
        return {
            "capture_running": await self._capture_running(),
            "snapserver_running": await self._snapserver_running(),
        }
```

---

## 6. PHASE 4 — CONFIGURATION TEMPLATES

### 6.1 Create `configs/snapserver.conf.template`

```ini
# SynchroSonic — snapserver configuration
# Generated by SynchroSonic setup. Manual edits may be overwritten.

[server]
threads = -1

[stream]
bind_to_address = 0.0.0.0
port = 1704

[http]
enabled = true
port = 1780

[tcp]
enabled = true
port = 1705

[stream]
# Pipe stream: reads raw PCM from FIFO at 48kHz/16-bit/stereo
source = pipe:///{{FIFO_PATH}}?name=SystemAudio&sampleformat=48000:16:2&codec=pcm

[logging]
enabled = true
sink = system
filter = *:info

[server]
datadir = {{DATA_DIR}}
```

**Substitution variables** (replace at setup time):
- `{{FIFO_PATH}}` → value of `FIFO_PATH` constant
- `{{DATA_DIR}}` → value of `DATA_DIR` constant

### 6.2 Create `configs/systemd/synchrosonic-capture.service.template`

```ini
[Unit]
Description=SynchroSonic audio capture (PipeWire → FIFO)
After=pipewire.service pipewire-pulse.service
Requires=pipewire.service

[Service]
Type=simple
ExecStart={{PW_RECORD_BIN}} --target {{MONITOR_SOURCE}} --channels 2 --rate 48000 --format s16 {{FIFO_PATH}}
Restart=on-failure
RestartSec=3
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=default.target
```

### 6.3 Create `configs/systemd/synchrosonic-snapserver.service.template`

```ini
[Unit]
Description=SynchroSonic snapserver
After=synchrosonic-capture.service
Requires=synchrosonic-capture.service

[Service]
Type=simple
ExecStart={{SNAPSERVER_BIN}} --config {{SNAPSERVER_CONF}}
Restart=on-failure
RestartSec=3
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=default.target
```

---

## 7. PHASE 5 — SCRIPTS

### 7.1 Create `scripts/install_deps_ubuntu.sh`

```bash
#!/usr/bin/env bash
set -euo pipefail

echo "==> SynchroSonic: installing dependencies on Ubuntu/Debian"

REQUIRED_PKGS=(
    snapserver
    snapclient
    pipewire
    pipewire-pulse
    pipewire-alsa
    wireplumber
    python3
    python3-pip
    python3-venv
    python3-dev
    libgl1
    libegl1
    avahi-daemon
    avahi-utils
)

sudo apt-get update -qq
sudo apt-get install -y "${REQUIRED_PKGS[@]}"

# Enable PipeWire as PulseAudio replacement (Ubuntu 22.04+)
systemctl --user enable --now pipewire pipewire-pulse wireplumber 2>/dev/null || true

# Verify snapserver is available
if ! command -v snapserver &>/dev/null; then
    echo "ERROR: snapserver not found after install. Check apt sources."
    exit 1
fi

echo "==> Dependencies installed."
echo "    Next step: run scripts/setup_server.sh"
```

### 7.2 Create `scripts/setup_server.sh`

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/synchrosonic"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/synchrosonic"
SYSTEMD_DIR="$HOME/.config/systemd/user"

echo "==> SynchroSonic server setup"

# Create directories
mkdir -p "$CONFIG_DIR" "$DATA_DIR" "$SYSTEMD_DIR"

# Create FIFO
FIFO="$DATA_DIR/system-audio.fifo"
if [ ! -p "$FIFO" ]; then
    mkfifo "$FIFO"
    echo "    Created FIFO: $FIFO"
fi

# Detect default PipeWire monitor source
DEFAULT_SINK=$(pactl get-default-sink 2>/dev/null || echo "")
if [ -z "$DEFAULT_SINK" ]; then
    echo "WARNING: Could not detect default audio sink. You can set the monitor source in the GUI."
    MONITOR_SOURCE="auto"
else
    MONITOR_SOURCE="${DEFAULT_SINK}.monitor"
    echo "    Detected monitor source: $MONITOR_SOURCE"
fi

# Generate snapserver.conf from template
CONF_TEMPLATE="$REPO_ROOT/configs/snapserver.conf.template"
CONF_OUT="$CONFIG_DIR/snapserver.conf"
sed \
    -e "s|{{FIFO_PATH}}|$FIFO|g" \
    -e "s|{{DATA_DIR}}|$DATA_DIR|g" \
    "$CONF_TEMPLATE" > "$CONF_OUT"
echo "    Generated: $CONF_OUT"

# Generate systemd service files from templates
PW_RECORD_BIN="$(command -v pw-record)"
SNAPSERVER_BIN="$(command -v snapserver)"

for svc in synchrosonic-capture synchrosonic-snapserver; do
    TMPL="$REPO_ROOT/configs/systemd/${svc}.service.template"
    OUT="$SYSTEMD_DIR/${svc}.service"
    sed \
        -e "s|{{PW_RECORD_BIN}}|$PW_RECORD_BIN|g" \
        -e "s|{{SNAPSERVER_BIN}}|$SNAPSERVER_BIN|g" \
        -e "s|{{MONITOR_SOURCE}}|$MONITOR_SOURCE|g" \
        -e "s|{{FIFO_PATH}}|$FIFO|g" \
        -e "s|{{SNAPSERVER_CONF}}|$CONF_OUT|g" \
        "$TMPL" > "$OUT"
    echo "    Generated: $OUT"
done

systemctl --user daemon-reload
echo "==> Server setup complete. Services are NOT started yet."
echo "    Use the SynchroSonic GUI or run:"
echo "      systemctl --user start synchrosonic-capture synchrosonic-snapserver"
```

### 7.3 Create `scripts/setup_client.sh`

```bash
#!/usr/bin/env bash
set -euo pipefail

SERVER_IP="${1:-}"

if [ -z "$SERVER_IP" ]; then
    echo "Usage: $0 <server_ip>"
    echo "  Example: $0 192.168.1.100"
    exit 1
fi

echo "==> Setting up SynchroSonic client pointing to $SERVER_IP"

sudo apt-get install -y snapclient

SYSTEMD_DIR="$HOME/.config/systemd/user"
mkdir -p "$SYSTEMD_DIR"

SNAPCLIENT_BIN="$(command -v snapclient)"

cat > "$SYSTEMD_DIR/synchrosonic-client.service" <<EOF
[Unit]
Description=SynchroSonic snapclient
After=network.target sound.target

[Service]
Type=simple
ExecStart=$SNAPCLIENT_BIN --host $SERVER_IP --port 1704
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload
systemctl --user enable --now synchrosonic-client.service

echo "==> Client setup complete. snapclient is running."
echo ""
echo "    Bluetooth output: pair your Bluetooth speaker on this device,"
echo "    then set it as the default audio output. snapclient will"
echo "    automatically route to whatever sink is default."
```

---

## 8. PHASE 6 — GUI APPLICATION

### 8.1 Create `gui/app.py`

```python
"""
SynchroSonic GUI entry point.
Run with: python -m gui.app   OR   synchrosonic (if installed)
"""

import sys
import asyncio
import qasync
from PySide6.QtWidgets import QApplication
from PySide6.QtCore import Qt
from gui.main_window import MainWindow
from synchrosonic.core.logger import setup_logging
from synchrosonic.core.config import load_config


def main() -> None:
    setup_logging()
    config = load_config()

    app = QApplication(sys.argv)
    app.setApplicationName("SynchroSonic")
    app.setApplicationVersion("0.1.0")
    app.setOrganizationName("SynchroSonic")

    # Load stylesheet
    import importlib.resources
    try:
        qss_path = importlib.resources.files("gui.assets").joinpath("style.qss")
        app.setStyleSheet(qss_path.read_text())
    except Exception:
        pass  # Stylesheet is cosmetic; proceed without it

    loop = qasync.QEventLoop(app)
    asyncio.set_event_loop(loop)

    window = MainWindow(config=config)
    window.show()

    with loop:
        loop.run_forever()


if __name__ == "__main__":
    main()
```

### 8.2 Create `gui/main_window.py`

```python
"""
Main application window with left-side navigation and page stack.
"""

import asyncio
from PySide6.QtWidgets import (
    QMainWindow, QWidget, QHBoxLayout, QVBoxLayout,
    QPushButton, QStackedWidget, QLabel, QSizePolicy,
    QSplitter,
)
from PySide6.QtCore import Qt, QTimer
from daemon.process_manager import ProcessManager
from daemon.rpc_client import SnapRPCClient
from gui.pages.dashboard import DashboardPage
from gui.pages.devices import DevicesPage
from gui.pages.groups import GroupsPage
from gui.pages.calibration import CalibrationPage
from gui.widgets.log_panel import LogPanel
from synchrosonic.core.logger import get_logger

logger = get_logger(__name__)

NAV_ITEMS = [
    ("Dashboard", 0),
    ("Devices", 1),
    ("Groups", 2),
    ("Calibration", 3),
]


class MainWindow(QMainWindow):
    def __init__(self, config: dict, parent=None):
        super().__init__(parent)
        self.config = config
        self.process_manager = ProcessManager()
        self.rpc_client = SnapRPCClient()

        self.setWindowTitle("SynchroSonic")
        self.setMinimumSize(900, 620)

        self._build_ui()
        self._start_status_poll()

    def _build_ui(self) -> None:
        central = QWidget()
        self.setCentralWidget(central)
        root_layout = QHBoxLayout(central)
        root_layout.setContentsMargins(0, 0, 0, 0)
        root_layout.setSpacing(0)

        # Left nav bar
        nav = self._build_nav()
        root_layout.addWidget(nav)

        # Main content area with log panel at bottom
        right = QSplitter(Qt.Vertical)
        self.pages = QStackedWidget()
        self.dashboard_page = DashboardPage(
            config=self.config,
            process_manager=self.process_manager,
            rpc_client=self.rpc_client,
        )
        self.devices_page = DevicesPage(rpc_client=self.rpc_client)
        self.groups_page = GroupsPage(rpc_client=self.rpc_client)
        self.calibration_page = CalibrationPage(rpc_client=self.rpc_client)

        self.pages.addWidget(self.dashboard_page)
        self.pages.addWidget(self.devices_page)
        self.pages.addWidget(self.groups_page)
        self.pages.addWidget(self.calibration_page)

        self.log_panel = LogPanel()
        right.addWidget(self.pages)
        right.addWidget(self.log_panel)
        right.setSizes([480, 140])

        root_layout.addWidget(right, stretch=1)

    def _build_nav(self) -> QWidget:
        nav = QWidget()
        nav.setFixedWidth(160)
        nav.setObjectName("NavBar")
        layout = QVBoxLayout(nav)
        layout.setContentsMargins(8, 16, 8, 16)
        layout.setSpacing(4)

        title = QLabel("SynchroSonic")
        title.setObjectName("AppTitle")
        layout.addWidget(title)
        layout.addSpacing(12)

        self._nav_buttons: list[QPushButton] = []
        for label, idx in NAV_ITEMS:
            btn = QPushButton(label)
            btn.setCheckable(True)
            btn.setObjectName("NavButton")
            btn.clicked.connect(lambda checked, i=idx: self._navigate(i))
            layout.addWidget(btn)
            self._nav_buttons.append(btn)

        self._nav_buttons[0].setChecked(True)
        layout.addStretch()
        return nav

    def _navigate(self, index: int) -> None:
        self.pages.setCurrentIndex(index)
        for i, btn in enumerate(self._nav_buttons):
            btn.setChecked(i == index)

    def _start_status_poll(self) -> None:
        self._poll_timer = QTimer(self)
        self._poll_timer.setInterval(3000)
        self._poll_timer.timeout.connect(self._poll_status)
        self._poll_timer.start()

    def _poll_status(self) -> None:
        asyncio.ensure_future(self._async_poll_status())

    async def _async_poll_status(self) -> None:
        try:
            status = await self.process_manager.status()
            self.dashboard_page.update_status(status)
        except Exception as e:
            logger.debug("Status poll error: %s", e)
```

### 8.3 Create `gui/pages/dashboard.py`

```python
"""
Dashboard page — "Start Casting" button, source selector, status indicators.
"""

import asyncio
from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QPushButton,
    QLabel, QGroupBox, QFrame,
)
from PySide6.QtCore import Qt
from daemon.process_manager import ProcessManager
from daemon.rpc_client import SnapRPCClient
from daemon.pipewire import list_monitor_sources
from gui.widgets.source_selector import SourceSelectorWidget
from synchrosonic.core.config import save_config
from synchrosonic.core.logger import get_logger

logger = get_logger(__name__)


class DashboardPage(QWidget):
    def __init__(self, config: dict, process_manager: ProcessManager, rpc_client: SnapRPCClient, parent=None):
        super().__init__(parent)
        self.config = config
        self.pm = process_manager
        self.rpc = rpc_client
        self._casting = False
        self._build_ui()
        asyncio.ensure_future(self._load_sources())

    def _build_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(24, 24, 24, 24)
        layout.setSpacing(16)

        # Header
        header = QLabel("Dashboard")
        header.setObjectName("PageTitle")
        layout.addWidget(header)

        # Source selection
        src_box = QGroupBox("Audio source")
        src_layout = QVBoxLayout(src_box)
        self.source_selector = SourceSelectorWidget()
        self.source_selector.source_changed.connect(self._on_source_changed)
        src_layout.addWidget(self.source_selector)
        layout.addWidget(src_box)

        # Status
        status_box = QGroupBox("Status")
        status_layout = QVBoxLayout(status_box)
        self.status_capture = QLabel("Capture: stopped")
        self.status_server = QLabel("snapserver: stopped")
        self.status_clients = QLabel("Clients: —")
        status_layout.addWidget(self.status_capture)
        status_layout.addWidget(self.status_server)
        status_layout.addWidget(self.status_clients)
        layout.addWidget(status_box)

        # Controls
        btn_row = QHBoxLayout()
        self.cast_btn = QPushButton("▶  Start Casting")
        self.cast_btn.setObjectName("PrimaryButton")
        self.cast_btn.setFixedHeight(44)
        self.cast_btn.clicked.connect(self._toggle_casting)

        self.stop_btn = QPushButton("■  Stop")
        self.stop_btn.setObjectName("DangerButton")
        self.stop_btn.setFixedHeight(44)
        self.stop_btn.setEnabled(False)
        self.stop_btn.clicked.connect(self._toggle_casting)

        btn_row.addWidget(self.cast_btn)
        btn_row.addWidget(self.stop_btn)
        layout.addLayout(btn_row)
        layout.addStretch()

    async def _load_sources(self) -> None:
        sources = await list_monitor_sources()
        self.source_selector.set_sources(sources)
        saved = self.config.get("audio", {}).get("monitor_source", "")
        if saved:
            self.source_selector.select_by_name(saved)

    def _on_source_changed(self, source_name: str) -> None:
        self.config.setdefault("audio", {})["monitor_source"] = source_name
        save_config(self.config)

    def _toggle_casting(self) -> None:
        asyncio.ensure_future(self._async_toggle())

    async def _async_toggle(self) -> None:
        if not self._casting:
            await self._start_casting()
        else:
            await self._stop_casting()

    async def _start_casting(self) -> None:
        source = self.source_selector.current_source_name()
        if not source:
            self.status_capture.setText("Capture: ERROR — no source selected")
            return
        self.cast_btn.setEnabled(False)
        self.status_capture.setText("Capture: starting…")
        try:
            await self.pm.start_snapserver()
            await asyncio.sleep(0.8)   # Let snapserver bind before starting capture
            await self.pm.start_capture(source)
            self._casting = True
            self.cast_btn.setEnabled(False)
            self.stop_btn.setEnabled(True)
        except Exception as e:
            logger.error("Failed to start casting: %s", e)
            self.status_capture.setText(f"Capture: ERROR — {e}")
            self.cast_btn.setEnabled(True)

    async def _stop_casting(self) -> None:
        self.stop_btn.setEnabled(False)
        await self.pm.stop_capture()
        await self.pm.stop_snapserver()
        self._casting = False
        self.cast_btn.setEnabled(True)
        self.status_capture.setText("Capture: stopped")
        self.status_server.setText("snapserver: stopped")

    def update_status(self, status: dict) -> None:
        self.status_capture.setText(
            "Capture: running" if status.get("capture_running") else "Capture: stopped"
        )
        self.status_server.setText(
            "snapserver: running" if status.get("snapserver_running") else "snapserver: stopped"
        )
```

### 8.4 Create `gui/pages/devices.py`

```python
"""
Devices page — lists connected snapclients with per-device volume, delay, mute.
"""

import asyncio
from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QScrollArea, QLabel, QPushButton, QHBoxLayout,
)
from PySide6.QtCore import Qt, QTimer
from daemon.rpc_client import SnapRPCClient
from gui.widgets.client_card import ClientCard
from synchrosonic.core.logger import get_logger

logger = get_logger(__name__)


class DevicesPage(QWidget):
    def __init__(self, rpc_client: SnapRPCClient, parent=None):
        super().__init__(parent)
        self.rpc = rpc_client
        self._cards: dict[str, ClientCard] = {}
        self._build_ui()
        self._start_refresh()

    def _build_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(24, 24, 24, 24)
        layout.setSpacing(12)

        header_row = QHBoxLayout()
        header = QLabel("Devices")
        header.setObjectName("PageTitle")
        header_row.addWidget(header)
        header_row.addStretch()
        refresh_btn = QPushButton("⟳ Refresh")
        refresh_btn.clicked.connect(lambda: asyncio.ensure_future(self._refresh()))
        header_row.addWidget(refresh_btn)
        layout.addLayout(header_row)

        self.no_clients_label = QLabel("No clients connected. Start casting and connect snapclient on receiver devices.")
        self.no_clients_label.setWordWrap(True)
        self.no_clients_label.setObjectName("EmptyState")
        layout.addWidget(self.no_clients_label)

        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(scroll.Shape.NoFrame)
        self.cards_container = QWidget()
        self.cards_layout = QVBoxLayout(self.cards_container)
        self.cards_layout.setSpacing(8)
        self.cards_layout.addStretch()
        scroll.setWidget(self.cards_container)
        layout.addWidget(scroll, stretch=1)

    def _start_refresh(self) -> None:
        self._timer = QTimer(self)
        self._timer.setInterval(4000)
        self._timer.timeout.connect(lambda: asyncio.ensure_future(self._refresh()))
        self._timer.start()

    async def _refresh(self) -> None:
        try:
            if not self.rpc._writer:
                await self.rpc.connect()
            clients = await self.rpc.get_clients()
        except Exception as e:
            logger.debug("Device refresh failed: %s", e)
            return

        self.no_clients_label.setVisible(len(clients) == 0)
        existing_ids = set(self._cards.keys())
        new_ids = {c.id for c in clients}

        # Remove stale cards
        for cid in existing_ids - new_ids:
            card = self._cards.pop(cid)
            self.cards_layout.removeWidget(card)
            card.deleteLater()

        # Add/update cards
        for client in clients:
            if client.id in self._cards:
                self._cards[client.id].update_client(client)
            else:
                card = ClientCard(client=client, rpc_client=self.rpc)
                self._cards[client.id] = card
                self.cards_layout.insertWidget(self.cards_layout.count() - 1, card)
```

### 8.5 Create `gui/widgets/client_card.py`

```python
"""
Widget representing a single snapclient device.
Shows name, IP, connection state, volume slider, delay slider, mute toggle.
"""

import asyncio
from PySide6.QtWidgets import (
    QFrame, QHBoxLayout, QVBoxLayout, QLabel, QSlider, QPushButton,
    QCheckBox,
)
from PySide6.QtCore import Qt
from daemon.rpc_client import SnapRPCClient
from synchrosonic.models.client import SnapClient
from synchrosonic.core.logger import get_logger

logger = get_logger(__name__)


class ClientCard(QFrame):
    def __init__(self, client: SnapClient, rpc_client: SnapRPCClient, parent=None):
        super().__init__(parent)
        self.client = client
        self.rpc = rpc_client
        self.setObjectName("ClientCard")
        self.setFrameShape(QFrame.Shape.StyledPanel)
        self._build_ui()

    def _build_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(16, 12, 16, 12)
        layout.setSpacing(8)

        # Header row
        header = QHBoxLayout()
        self.name_label = QLabel(self.client.name)
        self.name_label.setObjectName("ClientName")
        self.ip_label = QLabel(self.client.ip)
        self.ip_label.setObjectName("ClientIP")
        self.status_label = QLabel("● Connected" if self.client.connected else "○ Disconnected")
        self.status_label.setObjectName("Connected" if self.client.connected else "Disconnected")
        header.addWidget(self.name_label)
        header.addWidget(self.ip_label)
        header.addStretch()
        header.addWidget(self.status_label)
        layout.addLayout(header)

        # Volume row
        vol_row = QHBoxLayout()
        vol_row.addWidget(QLabel("Volume"))
        self.volume_slider = QSlider(Qt.Horizontal)
        self.volume_slider.setRange(0, 100)
        self.volume_slider.setValue(self.client.volume)
        self.volume_slider.setTracking(False)
        self.volume_slider.valueChanged.connect(self._on_volume_changed)
        self.vol_value = QLabel(f"{self.client.volume}%")
        self.mute_btn = QPushButton("Mute" if not self.client.muted else "Unmute")
        self.mute_btn.setCheckable(True)
        self.mute_btn.setChecked(self.client.muted)
        self.mute_btn.clicked.connect(self._on_mute_toggled)
        vol_row.addWidget(self.volume_slider, stretch=1)
        vol_row.addWidget(self.vol_value)
        vol_row.addWidget(self.mute_btn)
        layout.addLayout(vol_row)

        # Delay row
        delay_row = QHBoxLayout()
        delay_row.addWidget(QLabel("Delay (ms)"))
        self.delay_slider = QSlider(Qt.Horizontal)
        self.delay_slider.setRange(0, 2000)
        self.delay_slider.setValue(self.client.latency_ms)
        self.delay_slider.setTracking(False)
        self.delay_slider.valueChanged.connect(self._on_delay_changed)
        self.delay_value = QLabel(f"{self.client.latency_ms} ms")
        delay_row.addWidget(self.delay_slider, stretch=1)
        delay_row.addWidget(self.delay_value)
        layout.addLayout(delay_row)

        # Output note
        note_row = QHBoxLayout()
        note_row.addWidget(QLabel("Output:"))
        self.output_note = QLabel(self.client.output_note)
        self.output_note.setObjectName("OutputNote")
        note_row.addWidget(self.output_note)
        note_row.addStretch()
        layout.addLayout(note_row)

    def update_client(self, client: SnapClient) -> None:
        self.client = client
        self.name_label.setText(client.name)
        self.ip_label.setText(client.ip)
        self.status_label.setText("● Connected" if client.connected else "○ Disconnected")
        if not self.volume_slider.isSliderDown():
            self.volume_slider.setValue(client.volume)
        if not self.delay_slider.isSliderDown():
            self.delay_slider.setValue(client.latency_ms)

    def _on_volume_changed(self, value: int) -> None:
        self.vol_value.setText(f"{value}%")
        asyncio.ensure_future(
            self.rpc.set_client_volume(self.client.id, value, self.client.muted)
        )

    def _on_mute_toggled(self, checked: bool) -> None:
        self.client.muted = checked
        self.mute_btn.setText("Unmute" if checked else "Mute")
        asyncio.ensure_future(
            self.rpc.set_client_volume(self.client.id, self.client.volume, checked)
        )

    def _on_delay_changed(self, value: int) -> None:
        self.delay_value.setText(f"{value} ms")
        asyncio.ensure_future(
            self.rpc.set_client_latency(self.client.id, value)
        )
```

### 8.6 Create `gui/widgets/source_selector.py`

```python
"""
Dropdown widget for selecting a PipeWire monitor source.
"""

from PySide6.QtWidgets import QWidget, QHBoxLayout, QComboBox, QLabel
from PySide6.QtCore import Signal
from daemon.pipewire import MonitorSource


class SourceSelectorWidget(QWidget):
    source_changed = Signal(str)

    def __init__(self, parent=None):
        super().__init__(parent)
        layout = QHBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.addWidget(QLabel("Monitor source:"))
        self.combo = QComboBox()
        self.combo.setMinimumWidth(320)
        self.combo.currentIndexChanged.connect(self._on_changed)
        layout.addWidget(self.combo, stretch=1)
        self._sources: list[MonitorSource] = []

    def set_sources(self, sources: list[MonitorSource]) -> None:
        self._sources = sources
        self.combo.blockSignals(True)
        self.combo.clear()
        for s in sources:
            self.combo.addItem(s.description, userData=s.name)
        self.combo.blockSignals(False)

    def select_by_name(self, name: str) -> None:
        for i, s in enumerate(self._sources):
            if s.name == name:
                self.combo.setCurrentIndex(i)
                return

    def current_source_name(self) -> str:
        return self.combo.currentData() or ""

    def _on_changed(self, index: int) -> None:
        if index >= 0:
            self.source_changed.emit(self._sources[index].name)
```

### 8.7 Create `gui/widgets/log_panel.py`

```python
"""
Scrollable log viewer that attaches to Python's logging system.
"""

import logging
from PySide6.QtWidgets import QWidget, QVBoxLayout, QTextEdit, QHBoxLayout, QPushButton, QLabel
from PySide6.QtGui import QTextCursor
from PySide6.QtCore import Qt, Signal, QObject


class _QtLogHandler(logging.Handler, QObject):
    """Python log handler that emits a Qt signal per log record."""
    message_ready = Signal(str, int)

    def __init__(self):
        logging.Handler.__init__(self)
        QObject.__init__(self)

    def emit(self, record: logging.LogRecord) -> None:
        msg = self.format(record)
        self.message_ready.emit(msg, record.levelno)


class LogPanel(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setMaximumHeight(180)
        layout = QVBoxLayout(self)
        layout.setContentsMargins(8, 4, 8, 4)
        layout.setSpacing(4)

        header = QHBoxLayout()
        header.addWidget(QLabel("Logs"))
        clear_btn = QPushButton("Clear")
        clear_btn.setFixedWidth(60)
        clear_btn.clicked.connect(self._clear)
        header.addStretch()
        header.addWidget(clear_btn)
        layout.addLayout(header)

        self.text = QTextEdit()
        self.text.setReadOnly(True)
        self.text.setObjectName("LogText")
        layout.addWidget(self.text)

        self._handler = _QtLogHandler()
        self._handler.setFormatter(logging.Formatter("%(asctime)s [%(levelname)s] %(name)s: %(message)s", "%H:%M:%S"))
        self._handler.message_ready.connect(self._append)
        logging.getLogger().addHandler(self._handler)

    def _append(self, message: str, level: int) -> None:
        color_map = {
            logging.DEBUG: "#888",
            logging.INFO: "inherit",
            logging.WARNING: "#c8a000",
            logging.ERROR: "#d04040",
            logging.CRITICAL: "#a00020",
        }
        color = color_map.get(level, "inherit")
        self.text.append(f'<span style="color:{color};font-family:monospace;font-size:11px">{message}</span>')
        self.text.moveCursor(QTextCursor.End)

    def _clear(self) -> None:
        self.text.clear()
```

### 8.8 Create `gui/pages/groups.py`

```python
"""Groups page — create groups, assign clients, set group volume."""

import asyncio
from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QPushButton,
    QListWidget, QSlider, QGroupBox, QInputDialog,
)
from PySide6.QtCore import Qt
from daemon.rpc_client import SnapRPCClient
from synchrosonic.core.logger import get_logger

logger = get_logger(__name__)


class GroupsPage(QWidget):
    def __init__(self, rpc_client: SnapRPCClient, parent=None):
        super().__init__(parent)
        self.rpc = rpc_client
        self._build_ui()
        asyncio.ensure_future(self._refresh())

    def _build_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(24, 24, 24, 24)
        layout.setSpacing(16)

        header = QLabel("Groups")
        header.setObjectName("PageTitle")
        layout.addWidget(header)

        refresh_btn = QPushButton("⟳ Refresh")
        refresh_btn.setFixedWidth(120)
        refresh_btn.clicked.connect(lambda: asyncio.ensure_future(self._refresh()))
        layout.addWidget(refresh_btn)

        self.groups_box = QGroupBox("Active groups")
        groups_layout = QVBoxLayout(self.groups_box)
        self.groups_list = QListWidget()
        groups_layout.addWidget(self.groups_list)

        vol_row = QHBoxLayout()
        vol_row.addWidget(QLabel("Group volume:"))
        self.group_vol_slider = QSlider(Qt.Horizontal)
        self.group_vol_slider.setRange(0, 100)
        self.group_vol_slider.setValue(100)
        self.group_vol_slider.setTracking(False)
        self.group_vol_slider.valueChanged.connect(self._on_group_volume)
        self.group_vol_label = QLabel("100%")
        vol_row.addWidget(self.group_vol_slider, stretch=1)
        vol_row.addWidget(self.group_vol_label)
        groups_layout.addLayout(vol_row)
        layout.addWidget(self.groups_box)
        layout.addStretch()

    async def _refresh(self) -> None:
        try:
            if not self.rpc._writer:
                await self.rpc.connect()
            groups = await self.rpc.get_groups()
            self.groups_list.clear()
            for g in groups:
                self.groups_list.addItem(f"{g.name} ({len(g.client_ids)} clients) — vol {g.volume}%")
            self._groups = groups
        except Exception as e:
            logger.debug("Groups refresh error: %s", e)

    def _on_group_volume(self, value: int) -> None:
        self.group_vol_label.setText(f"{value}%")
        item = self.groups_list.currentItem()
        if item is None or not hasattr(self, "_groups"):
            return
        idx = self.groups_list.currentRow()
        if idx < len(self._groups):
            asyncio.ensure_future(
                self.rpc.set_group_volume(self._groups[idx].id, value)
            )
```

### 8.9 Create `gui/pages/calibration.py`

```python
"""Calibration page — test tone generator and per-client latency tuning."""

import asyncio
import subprocess
from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QPushButton,
    QSlider, QGroupBox, QSpinBox,
)
from PySide6.QtCore import Qt
from daemon.rpc_client import SnapRPCClient
from synchrosonic.core.constants import TEST_TONE_FREQ_HZ, TEST_TONE_DURATION_S
from synchrosonic.core.logger import get_logger

logger = get_logger(__name__)


class CalibrationPage(QWidget):
    def __init__(self, rpc_client: SnapRPCClient, parent=None):
        super().__init__(parent)
        self.rpc = rpc_client
        self._build_ui()

    def _build_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(24, 24, 24, 24)
        layout.setSpacing(16)

        header = QLabel("Calibration")
        header.setObjectName("PageTitle")
        layout.addWidget(header)

        # Test tone
        tone_box = QGroupBox("Test tone")
        tone_layout = QVBoxLayout(tone_box)
        tone_info = QLabel(
            f"Plays a {TEST_TONE_FREQ_HZ} Hz sine wave for {TEST_TONE_DURATION_S} seconds "
            "through the active stream. Use this to check that all devices are receiving audio "
            "and to identify latency differences by ear."
        )
        tone_info.setWordWrap(True)
        tone_layout.addWidget(tone_info)

        tone_btn_row = QHBoxLayout()
        self.tone_btn = QPushButton("▶  Play test tone")
        self.tone_btn.setFixedHeight(40)
        self.tone_btn.clicked.connect(self._play_test_tone)
        tone_btn_row.addWidget(self.tone_btn)
        tone_btn_row.addStretch()
        tone_layout.addLayout(tone_btn_row)
        layout.addWidget(tone_box)

        # Global buffer
        buf_box = QGroupBox("Stream buffer")
        buf_layout = QHBoxLayout(buf_box)
        buf_layout.addWidget(QLabel("Buffer (ms):"))
        self.buffer_spin = QSpinBox()
        self.buffer_spin.setRange(100, 5000)
        self.buffer_spin.setSingleStep(50)
        self.buffer_spin.setValue(1000)
        self.buffer_spin.setToolTip(
            "Higher values improve sync stability at the cost of added latency. "
            "Recommended: 1000–2000ms."
        )
        buf_layout.addWidget(self.buffer_spin)
        buf_layout.addStretch()
        layout.addWidget(buf_box)

        layout.addStretch()

    def _play_test_tone(self) -> None:
        """Generate and play a test tone using `speaker-test` or `sox`."""
        self.tone_btn.setEnabled(False)
        self.tone_btn.setText("Playing…")
        asyncio.ensure_future(self._async_tone())

    async def _async_tone(self) -> None:
        try:
            # Try sox first (gplay sine wave to default output)
            proc = await asyncio.create_subprocess_exec(
                "sox", "-n", "-d",
                "synth", str(TEST_TONE_DURATION_S), "sine", str(TEST_TONE_FREQ_HZ),
                "vol", "0.5",
                stderr=asyncio.subprocess.DEVNULL,
                stdout=asyncio.subprocess.DEVNULL,
            )
            await proc.wait()
        except FileNotFoundError:
            # Fallback: speaker-test
            try:
                proc = await asyncio.create_subprocess_exec(
                    "speaker-test", "-t", "sine", "-f", str(TEST_TONE_FREQ_HZ),
                    "-l", "1",
                    stderr=asyncio.subprocess.DEVNULL,
                    stdout=asyncio.subprocess.DEVNULL,
                )
                await asyncio.wait_for(proc.wait(), timeout=TEST_TONE_DURATION_S + 2)
            except Exception as e:
                logger.error("Test tone failed: %s", e)
        finally:
            self.tone_btn.setEnabled(True)
            self.tone_btn.setText("▶  Play test tone")
```

### 8.10 Create `gui/assets/style.qss`

```css
QMainWindow, QWidget {
    background-color: #1e1e2e;
    color: #cdd6f4;
    font-family: "Inter", "Segoe UI", sans-serif;
    font-size: 13px;
}

#NavBar {
    background-color: #181825;
    border-right: 1px solid #313244;
}

#AppTitle {
    font-size: 14px;
    font-weight: bold;
    color: #cba6f7;
    padding: 4px 8px;
}

#NavButton {
    background: transparent;
    border: none;
    border-radius: 6px;
    padding: 8px 12px;
    text-align: left;
    color: #a6adc8;
}

#NavButton:checked, #NavButton:hover {
    background-color: #313244;
    color: #cdd6f4;
}

#PageTitle {
    font-size: 20px;
    font-weight: bold;
    color: #cdd6f4;
    margin-bottom: 8px;
}

#PrimaryButton {
    background-color: #89b4fa;
    color: #1e1e2e;
    border: none;
    border-radius: 8px;
    font-weight: bold;
    padding: 0 20px;
}

#PrimaryButton:hover { background-color: #74c7ec; }
#PrimaryButton:disabled { background-color: #45475a; color: #6c7086; }

#DangerButton {
    background-color: #f38ba8;
    color: #1e1e2e;
    border: none;
    border-radius: 8px;
    font-weight: bold;
    padding: 0 20px;
}

#DangerButton:disabled { background-color: #45475a; color: #6c7086; }

#ClientCard {
    background-color: #181825;
    border: 1px solid #313244;
    border-radius: 10px;
}

QGroupBox {
    border: 1px solid #313244;
    border-radius: 8px;
    padding-top: 16px;
    font-weight: bold;
}

QGroupBox::title {
    subcontrol-origin: margin;
    left: 12px;
    top: 4px;
    color: #a6adc8;
}

QSlider::groove:horizontal {
    height: 4px;
    background: #45475a;
    border-radius: 2px;
}

QSlider::handle:horizontal {
    width: 14px;
    height: 14px;
    margin: -5px 0;
    background: #89b4fa;
    border-radius: 7px;
}

QSlider::sub-page:horizontal { background: #89b4fa; border-radius: 2px; }

#LogText {
    background-color: #11111b;
    border: 1px solid #313244;
    border-radius: 6px;
    font-family: "JetBrains Mono", "Cascadia Code", monospace;
    font-size: 11px;
    color: #a6adc8;
}

#EmptyState { color: #6c7086; font-style: italic; }
#Connected   { color: #a6e3a1; }
#Disconnected { color: #6c7086; }
```

---

## 9. PHASE 7 — TESTS

### 9.1 Create `tests/test_config.py`

```python
import tempfile
from pathlib import Path
import pytest
from unittest.mock import patch


def test_save_and_load_config():
    with tempfile.TemporaryDirectory() as tmpdir:
        cfg_file = Path(tmpdir) / "config.toml"
        with patch("synchrosonic.core.config.CONFIG_FILE", cfg_file), \
             patch("synchrosonic.core.config.CONFIG_DIR", Path(tmpdir)):
            from synchrosonic.core.config import save_config, load_config
            data = {"server": {"buffer_ms": 1500}, "audio": {"monitor_source": "test.monitor"}}
            save_config(data)
            loaded = load_config()
            assert loaded["audio"]["monitor_source"] == "test.monitor"
            assert loaded["server"]["buffer_ms"] == 1500
```

### 9.2 Create `tests/test_rpc_client.py`

```python
import asyncio
import json
import pytest
from unittest.mock import AsyncMock, MagicMock, patch


@pytest.mark.asyncio
async def test_call_returns_result():
    from daemon.rpc_client import SnapRPCClient
    client = SnapRPCClient()
    mock_writer = AsyncMock()
    client._writer = mock_writer

    async def inject_response():
        await asyncio.sleep(0.05)
        # Manually inject a response into pending futures
        for req_id, fut in list(client._pending.items()):
            if not fut.done():
                fut.set_result({"status": "ok"})

    asyncio.create_task(inject_response())
    result = await client.call("Server.GetStatus")
    assert result == {"status": "ok"}
```

### 9.3 Create `tests/test_pipewire.py`

```python
import pytest
from unittest.mock import patch, AsyncMock


@pytest.mark.asyncio
async def test_list_sources_returns_empty_on_failure():
    with patch("daemon.pipewire._run", new_callable=AsyncMock) as mock_run:
        mock_run.return_value = (-1, "", "command not found")
        from daemon.pipewire import list_monitor_sources
        sources = await list_monitor_sources()
        assert isinstance(sources, list)  # Never raises; returns []


@pytest.mark.asyncio
async def test_pactl_fallback_parses_monitor_lines():
    pactl_output = "0\talsa_output.pci-0000_00_1f.3.analog-stereo.monitor\tPipeWire\t..."
    with patch("daemon.pipewire._run", new_callable=AsyncMock) as mock_run:
        mock_run.side_effect = [
            (-1, "", ""),            # pw-cli fails
            (0, pactl_output, ""),   # pactl succeeds
        ]
        from daemon.pipewire import list_monitor_sources
        sources = await list_monitor_sources()
        assert len(sources) == 1
        assert "monitor" in sources[0].name
```

---

## 10. PHASE 8 — DOCUMENTATION

### 10.1 Create `docs/architecture.md`

Write a complete architecture document covering:
- Component overview (PipeWire → FIFO → snapserver → LAN → snapclients)
- Why Bluetooth speakers cannot run snapclient (Bluetooth speakers are output-only devices; they run no OS and cannot execute arbitrary software. The Linux device *paired* to the Bluetooth speaker runs snapclient and routes audio to the paired Bluetooth sink via PulseAudio/PipeWire.)
- How sync works: snapserver embeds NTP-aligned timestamps in each audio chunk. Each snapclient maintains a local playback buffer (default 1000ms). If playback drifts beyond a configurable threshold, the client adjusts buffer depth in real time, avoiding audible artifacts.
- JSON-RPC API surface: list all methods used (Server.GetStatus, Client.SetVolume, Client.SetLatency, Client.SetName, Group.SetClients, Group.SetVolume)
- systemd user service topology
- Security model (see security.md)

### 10.2 Create `docs/security.md`

Cover:
- Default: no authentication on the snapserver control port (1705). This is safe on trusted home LANs.
- Proposed optional token: add a shared TOML config key `security.token`. The GUI daemon signs each JSON-RPC request with HMAC-SHA256 using this token. Clients that present the wrong or missing token are rejected.
- Network exposure: snapserver binds to 0.0.0.0 by default. Recommend firewall rule for LAN-only access.
- Implementation plan for token auth (mark as v0.3 feature).

### 10.3 Create `docs/troubleshooting.md`

Cover these exact scenarios with step-by-step resolution:
1. "No monitor sources found" — PipeWire not running; run `systemctl --user start pipewire`
2. "snapserver fails to start" — conf file missing or FIFO not created; run setup_server.sh again
3. "Audio plays but clients are out of sync" — increase buffer_ms; check NTP on all devices
4. "Bluetooth speaker produces no audio" — verify the Bluetooth sink is set as default on the receiver device using `pactl set-default-sink <bt-sink-name>`
5. "GUI crashes on launch" — missing PySide6; run `pip install -r requirements.txt`

---

## 11. PHASE 9 — CI PIPELINE

### 11.1 Create `.github/workflows/ci.yml`

```yaml
name: CI

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]

jobs:
  test:
    runs-on: ubuntu-22.04
    steps:
      - uses: actions/checkout@v4

      - name: Set up Python
        uses: actions/setup-python@v5
        with:
          python-version: "3.11"

      - name: Install system deps
        run: |
          sudo apt-get update -qq
          sudo apt-get install -y snapserver pipewire pipewire-pulse \
            libegl1 libgl1 libxkbcommon-x11-0 libxcb-icccm4 libxcb-image0 \
            libxcb-keysyms1 libxcb-randr0 libxcb-render-util0 libxcb-xinerama0 \
            libxcb-xfixes0 xvfb

      - name: Install Python deps
        run: pip install -r requirements.txt

      - name: Run tests
        run: |
          export DISPLAY=:99
          Xvfb :99 -screen 0 1024x768x24 &
          sleep 1
          pytest tests/ -v --tb=short

      - name: Lint
        run: |
          pip install ruff
          ruff check synchrosonic/ daemon/ gui/
```

---

## 12. PHASE 10 — PACKAGING

### 12.1 Create `packaging/deb/control`

```
Package: synchrosonic
Version: 0.1.0
Architecture: amd64
Maintainer: SynchroSonic Contributors <dev@synchrosonic.example>
Depends: python3 (>= 3.11), python3-pip, snapserver, pipewire, pipewire-pulse, libgl1
Description: Synchronized multi-room audio streaming for Linux
 SynchroSonic streams your Linux system audio in tight sync to
 multiple devices on the same LAN using PipeWire and Snapcast.
```

### 12.2 Create `packaging/deb/postinst`

```bash
#!/bin/bash
set -e
pip3 install --break-system-packages PySide6 qasync tomli-w zeroconf aiohttp
echo "SynchroSonic installed. Run 'synchrosonic' to launch."
```

---

## 13. FINAL README OUTLINE

Create `README.md` covering these sections in order:
1. Banner image placeholder
2. One-sentence description
3. Features list
4. System requirements (Ubuntu 22.04+, Python 3.11+, PipeWire)
5. Quickstart (clone → install_deps → setup_server → launch GUI)
6. Client setup (run setup_client.sh on each receiver)
7. Bluetooth output note
8. Screenshots (placeholder)
9. Architecture link
10. Contributing
11. License (GPLv3)

---

## 14. EXECUTION ORDER CHECKLIST

The agent must complete steps in this order. Check off each before proceeding.

- [ ] Phase 1: Scaffolding — all files in synchrosonic/core/ created and importable
- [ ] Phase 2: Models — SnapClient and SnapGroup parse from mock RPC dicts correctly
- [ ] Phase 3: Daemon — process_manager, rpc_client, pipewire all have complete implementations
- [ ] Phase 4: Config templates — all `{{PLACEHOLDER}}` variables documented
- [ ] Phase 5: Scripts — all three bash scripts pass `bash -n` (syntax check)
- [ ] Phase 6: GUI — app launches without errors (`python -m gui.app`)
- [ ] Phase 7: Tests — `pytest tests/ -v` passes (≥ 4 tests, 0 failures)
- [ ] Phase 8: Docs — all three markdown docs written with full content
- [ ] Phase 9: CI — workflow YAML is valid; no syntax errors
- [ ] Phase 10: Packaging — deb/control parses correctly with `dpkg-parsechangelog` or equivalent

---

## 15. KNOWN CONSTRAINTS & EDGE CASES

Handle these explicitly in your implementation — do not defer them:

| Scenario | Required handling |
|---|---|
| PipeWire not running at launch | Catch subprocess error in `pipewire.py`; show warning in GUI, do not crash |
| snapserver not installed | Check with `shutil.which(SNAPSERVER_BIN)` in `process_manager.py`; surface error in dashboard |
| FIFO path exists but is not a FIFO | Delete and recreate in `ensure_fifo()` |
| RPC client loses TCP connection mid-session | Implement reconnect with exponential backoff in `SnapRPCClient.call()` |
| User closes window while casting | Connect `QApplication.aboutToQuit` to `process_manager.stop_capture()` and `stop_snapserver()` |
| No audio sources returned | Show "No monitor sources found" in SourceSelectorWidget with a Retry button |
| Volume slider dragged rapidly | Use `setTracking(False)` and debounce with 150ms QTimer before sending RPC call |

---

*End of build brief. Total implementation scope: ~2,500 lines of production Python + configs + scripts.*
