#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
PACKAGE_ROOT="$ROOT/target/release-packaging"
CACHE_ROOT="$ROOT/target/release-artifacts-cache"
MANIFEST_PATH="$PACKAGE_ROOT/RELEASE_MANIFEST.json"
FLATPAK_RUNTIME_REPO="https://flathub.org/repo/flathub.flatpakrepo"
SKIP_FLATPAK="${SKIP_FLATPAK:-0}"

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

log() {
    printf '==> %s\n' "$*"
}

die() {
    printf 'ERROR: %s\n' "$*" >&2
    exit 1
}

sudo_if_needed() {
    if [[ "$(id -u)" -eq 0 ]]; then
        "$@"
        return
    fi

    if ! command -v sudo >/dev/null 2>&1; then
        die "system-scope Flatpak setup requires sudo or root"
    fi

    sudo "$@"
}

flatpak_native_tooling_available() {
    command -v flatpak >/dev/null 2>&1 && command -v flatpak-builder >/dev/null 2>&1
}

ensure_flatpak_tools() {
    command -v flatpak >/dev/null 2>&1 || die "flatpak is not installed"
    command -v flatpak-builder >/dev/null 2>&1 || die "flatpak-builder is not installed"
}

ensure_flathub_remote() {
    local refs_file err_file

    # flatpak-builder resolves runtime dependencies from the installation it is
    # operating against. The Actions runner previously had Flathub in the wrong
    # scope, which left the remote visible in one place but empty in the scope
    # that flatpak-builder used for dependency installation.
    log "Configuring Flathub remote in system scope"
    sudo_if_needed flatpak --system remote-delete flathub >/dev/null 2>&1 || true
    sudo_if_needed flatpak --system remote-add --if-not-exists flathub "$FLATPAK_RUNTIME_REPO"

    log "Flatpak diagnostics"
    flatpak --version || true
    sudo_if_needed flatpak remotes --system || true

    refs_file="$(mktemp "$CACHE_ROOT/flathub-refs.XXXXXX")"
    err_file="$(mktemp "$CACHE_ROOT/flathub-refs.err.XXXXXX")"

    if ! sudo_if_needed flatpak remote-ls --system flathub >"$refs_file" 2>"$err_file"; then
        cat "$err_file" >&2 || true
        rm -f "$refs_file" "$err_file"
        die "Unable to query Flathub refs in system scope"
    fi

    grep 'org.freedesktop' "$refs_file" | head -20 || true

    if [[ ! -s "$refs_file" ]]; then
        rm -f "$refs_file" "$err_file"
        die "Flathub remote exists but no refs are visible in system scope"
    fi

    rm -f "$refs_file" "$err_file"
}

install_flatpak_deps() {
    log "Installing Flatpak runtime and SDK"
    sudo_if_needed flatpak --system install -y --noninteractive flathub \
        org.freedesktop.Platform//24.08 \
        org.freedesktop.Sdk//24.08

    sudo_if_needed flatpak --system install -y --noninteractive flathub \
        org.freedesktop.Platform.GL.default//24.08 || true

    sudo_if_needed flatpak --system install -y --noninteractive flathub \
        org.freedesktop.Platform.Locale//24.08 || true
}

build_flatpak_bundle() {
    if ! flatpak_native_tooling_available; then
        log "Native Flatpak tooling not found; delegating to scripts/build-flatpak.sh fallback path"
        if ! bash "$ROOT/scripts/build-flatpak.sh" --skip-build; then
            die "Flatpak bundle build failed after AppImage, Debian, and tarball artifacts were already produced"
        fi
        return
    fi

    ensure_flatpak_tools
    ensure_flathub_remote
    install_flatpak_deps

    log "Building Flatpak bundle"
    if ! SYNCHROSONIC_FLATPAK_SKIP_DEP_INSTALL=1 bash "$ROOT/scripts/build-flatpak.sh" --skip-build; then
        die "Flatpak bundle build failed after AppImage, Debian, and tarball artifacts were already produced"
    fi
}

if [[ "$SKIP_BUILD" -eq 0 ]]; then
    cargo build --release -p synchrosonic-app
fi

version="$(python3 "$ROOT/scripts/read-workspace-version.py")"
arch="$(uname -m)"

if [[ "$arch" != "x86_64" ]]; then
    die "unsupported architecture: $arch"
fi

appimage_artifact="$PACKAGE_ROOT/synchrosonic-${version}-x86_64.AppImage"
cached_appimage="$CACHE_ROOT/$(basename "$appimage_artifact")"

rm -rf "$CACHE_ROOT"
mkdir -p "$CACHE_ROOT"

bash "$ROOT/scripts/build-appimage.sh" --skip-build
cp "$appimage_artifact" "$cached_appimage"

bash "$ROOT/scripts/build-deb.sh" --skip-build
cp "$cached_appimage" "$appimage_artifact"

if [[ "$SKIP_FLATPAK" == "1" ]]; then
    log "Skipping Flatpak because SKIP_FLATPAK=1"
else
    build_flatpak_bundle
fi

if command -v dpkg-scanpackages >/dev/null 2>&1; then
    bash "$ROOT/scripts/build-apt-repo.sh"
else
    printf 'dpkg-scanpackages not found; skipping the unsigned APT repository scaffold.\n'
fi

artifacts=(
    "$appimage_artifact"
    "$PACKAGE_ROOT/synchrosonic_${version}_amd64.deb"
    "$PACKAGE_ROOT/synchrosonic-${version}-linux-${arch}.tar.gz"
)
artifact_kinds=(
    "appimage"
    "debian-package"
    "portable-tarball"
)

if [[ "$SKIP_FLATPAK" != "1" ]]; then
    artifacts+=("$PACKAGE_ROOT/synchrosonic-${version}.flatpak")
    artifact_kinds+=("flatpak-bundle")
fi

for artifact in "${artifacts[@]}"; do
    if [[ ! -f "$artifact" ]]; then
        die "missing release artifact: $artifact"
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

SKIP_FLATPAK="$SKIP_FLATPAK" bash "$ROOT/scripts/verify-release-artifacts.sh"

printf 'Generated checksum manifest: %s/SHA256SUMS.txt\n' "$PACKAGE_ROOT"
printf 'Generated release manifest: %s\n' "$MANIFEST_PATH"
