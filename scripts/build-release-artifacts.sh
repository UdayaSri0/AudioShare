#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
PACKAGE_ROOT="$ROOT/target/release-packaging"
CACHE_ROOT="$ROOT/target/release-artifacts-cache"
MANIFEST_PATH="$PACKAGE_ROOT/RELEASE_MANIFEST.json"

SKIP_BUILD=0
for arg in "$@"; do
    case "$arg" in
        --skip-build)
            SKIP_BUILD=1
            ;;
        *)
            printf 'unknown argument: %s\n' "$arg" >&2
            exit 2
            ;;
    esac
done

if [[ "$SKIP_BUILD" -eq 0 ]]; then
    cargo build --release -p synchrosonic-app
fi

version="$(python3 "$ROOT/scripts/read-workspace-version.py")"
arch="$(uname -m)"

if [[ "$arch" != "x86_64" ]]; then
    printf 'unsupported architecture: %s\n' "$arch" >&2
    exit 1
fi

appimage_artifact="$PACKAGE_ROOT/synchrosonic-${version}-x86_64.AppImage"
cached_appimage="$CACHE_ROOT/$(basename "$appimage_artifact")"

rm -rf "$CACHE_ROOT"
mkdir -p "$CACHE_ROOT"

bash "$ROOT/scripts/build-appimage.sh" --skip-build
cp "$appimage_artifact" "$cached_appimage"

bash "$ROOT/scripts/build-deb.sh" --skip-build
cp "$cached_appimage" "$appimage_artifact"
bash "$ROOT/scripts/build-flatpak.sh"

if command -v dpkg-scanpackages >/dev/null 2>&1; then
    bash "$ROOT/scripts/build-apt-repo.sh"
else
    printf 'dpkg-scanpackages not found; skipping the unsigned APT repository scaffold.\n'
fi

artifacts=(
    "$appimage_artifact"
    "$PACKAGE_ROOT/synchrosonic_${version}_amd64.deb"
    "$PACKAGE_ROOT/synchrosonic-${version}.flatpak"
    "$PACKAGE_ROOT/synchrosonic-${version}-linux-${arch}.tar.gz"
)
artifact_kinds=(
    "appimage"
    "debian-package"
    "flatpak-bundle"
    "portable-tarball"
)

for artifact in "${artifacts[@]}"; do
    if [[ ! -f "$artifact" ]]; then
        printf 'missing release artifact: %s\n' "$artifact" >&2
        exit 1
    fi
done

manifest_input="$CACHE_ROOT/release-artifacts.tsv"
: >"$manifest_input"
for index in "${!artifacts[@]}"; do
    printf '%s\t%s\n' "${artifact_kinds[$index]}" "${artifacts[$index]}" >>"$manifest_input"
done

python3 - "$version" "$arch" "$PACKAGE_ROOT" "$manifest_input" "$MANIFEST_PATH" <<'PY'
from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

version, arch, package_root, manifest_input, manifest_path = sys.argv[1:]
package_root_path = Path(package_root)
manifest_entries = []
checksum_lines = []

with Path(manifest_input).open("r", encoding="utf-8") as handle:
    for raw_line in handle:
        kind, raw_path = raw_line.rstrip("\n").split("\t", 1)
        path = Path(raw_path)
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        filename = path.name
        checksum_lines.append(f"{digest}  {filename}")
        manifest_entries.append(
            {
                "kind": kind,
                "filename": filename,
                "size_bytes": path.stat().st_size,
                "sha256": digest,
            }
        )

(package_root_path / "SHA256SUMS.txt").write_text("\n".join(checksum_lines) + "\n", encoding="utf-8")
Path(manifest_path).write_text(
    json.dumps(
        {
            "product": "SynchroSonic",
            "version": version,
            "architecture": arch,
            "artifacts": manifest_entries,
        },
        indent=2,
    )
    + "\n",
    encoding="utf-8",
)
PY

bash "$ROOT/scripts/verify-release-artifacts.sh"

printf 'Generated checksum manifest: %s/SHA256SUMS.txt\n' "$PACKAGE_ROOT"
printf 'Generated release manifest: %s\n' "$MANIFEST_PATH"
