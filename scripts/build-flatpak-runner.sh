#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
PACKAGE_ROOT="$ROOT/target/release-packaging"
MANIFEST="$ROOT/packaging/flatpak/org.synchrosonic.SynchroSonic.yml"
BUILD_DIR="$PACKAGE_ROOT/flatpak-build"
REPO_DIR="$PACKAGE_ROOT/flatpak-repo"
RUNTIME_REPO="https://flathub.org/repo/flathub.flatpakrepo"
APP_ID="org.synchrosonic.SynchroSonic"
APP_BRANCH="${SYNCHROSONIC_FLATPAK_BRANCH:-stable}"
RUNTIME_VERSION="24.08"
FLATPAK_ARCH="${SYNCHROSONIC_FLATPAK_ARCH:-x86_64}"

die() {
    printf '%s\n' "$1" >&2
    exit 1
}

note() {
    printf '%s\n' "$1"
}

require_command() {
    local command_name="$1"
    local install_hint="$2"
    if ! command -v "$command_name" >/dev/null 2>&1; then
        die "${command_name} is required; ${install_hint}"
    fi
}

require_file() {
    local path="$1"
    local description="$2"
    if [[ ! -f "$path" ]]; then
        die "missing ${description}: $path"
    fi
}

export HOME="${SYNCHROSONIC_FLATPAK_HOME:-$HOME}"
export XDG_CACHE_HOME="${SYNCHROSONIC_FLATPAK_CACHE_HOME:-$HOME/.cache}"
export XDG_DATA_HOME="${SYNCHROSONIC_FLATPAK_DATA_HOME:-$HOME/.local/share}"

require_command flatpak "install flatpak before running the Flatpak builder"
require_command flatpak-builder "install flatpak-builder before running the Flatpak builder"
require_file "$MANIFEST" "Flatpak manifest"

version="${SYNCHROSONIC_WORKSPACE_VERSION:-}"
if [[ -z "$version" ]]; then
    require_command python3 "install Python 3 to read the workspace version"
    version="$(python3 "$ROOT/scripts/read-workspace-version.py")"
fi
BUNDLE="$PACKAGE_ROOT/synchrosonic-${version}.flatpak"

mkdir -p "$HOME" "$XDG_CACHE_HOME" "$XDG_DATA_HOME" "$PACKAGE_ROOT"
rm -rf "$BUILD_DIR" "$REPO_DIR" "$BUNDLE"

run_with_session_bus() {
    if command -v dbus-run-session >/dev/null 2>&1; then
        dbus-run-session -- "$@"
    else
        note "dbus-run-session not found; running Flatpak commands without a temporary session bus."
        "$@"
    fi
}

note "Ensuring Flatpak runtime and SDK from Flathub."
run_with_session_bus bash -lc "
    set -euo pipefail
    flatpak remote-add --user --if-not-exists flathub '$RUNTIME_REPO'
    flatpak install --user --noninteractive -y flathub 'org.freedesktop.Platform/$FLATPAK_ARCH/$RUNTIME_VERSION' 'org.freedesktop.Sdk/$FLATPAK_ARCH/$RUNTIME_VERSION'
    flatpak-builder --force-clean --install-deps-from=flathub --repo='$REPO_DIR' '$BUILD_DIR' '$MANIFEST'
    flatpak build-bundle '$REPO_DIR' '$BUNDLE' '$APP_ID' '$APP_BRANCH' --runtime-repo='$RUNTIME_REPO'
"

require_file "$BUNDLE" "Flatpak bundle"
printf 'Built Flatpak bundle: %s\n' "$BUNDLE"
