#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
PACKAGE_ROOT="$ROOT/target/release-packaging"
MANIFEST="$ROOT/packaging/flatpak/org.synchrosonic.SynchroSonic.yml"
BUILD_DIR="$PACKAGE_ROOT/flatpak-build"
REPO_DIR="$PACKAGE_ROOT/flatpak-repo"
RUNTIME_REPO="https://flathub.org/repo/flathub.flatpakrepo"
APP_ID="org.synchrosonic.SynchroSonic"
RUNTIME_VERSION="24.08"
version="$(python3 "$ROOT/scripts/read-workspace-version.py")"
BUNDLE="$PACKAGE_ROOT/synchrosonic-${version}.flatpak"

export HOME="${SYNCHROSONIC_FLATPAK_HOME:-$HOME}"
export XDG_CACHE_HOME="${SYNCHROSONIC_FLATPAK_CACHE_HOME:-$HOME/.cache}"
export XDG_DATA_HOME="${SYNCHROSONIC_FLATPAK_DATA_HOME:-$HOME/.local/share}"

mkdir -p "$HOME" "$XDG_CACHE_HOME" "$XDG_DATA_HOME" "$PACKAGE_ROOT"
rm -rf "$BUILD_DIR" "$REPO_DIR" "$BUNDLE"

run_with_session_bus() {
    if command -v dbus-run-session >/dev/null 2>&1; then
        dbus-run-session -- "$@"
    else
        "$@"
    fi
}

run_with_session_bus bash -lc "
    set -euo pipefail
    flatpak remote-add --user --if-not-exists flathub '$RUNTIME_REPO'
    flatpak install --user -y flathub org.freedesktop.Platform//$RUNTIME_VERSION org.freedesktop.Sdk//$RUNTIME_VERSION
    flatpak-builder --force-clean --install-deps-from=flathub --repo='$REPO_DIR' '$BUILD_DIR' '$MANIFEST'
    flatpak build-bundle '$REPO_DIR' '$BUNDLE' '$APP_ID' '$version' --runtime-repo='$RUNTIME_REPO'
"
