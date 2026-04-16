#!/usr/bin/env python3

from pathlib import Path
import tomllib


def main() -> None:
    root = Path(__file__).resolve().parents[1]
    cargo_toml = root / "Cargo.toml"
    data = tomllib.loads(cargo_toml.read_text(encoding="utf-8"))
    print(data["workspace"]["package"]["version"])


if __name__ == "__main__":
    main()
