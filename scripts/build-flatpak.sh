#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
PACKAGE_ROOT="$ROOT/target/release-packaging"
MANIFEST="$ROOT/packaging/flatpak/org.synchrosonic.SynchroSonic.yml"
BUILD_DIR="$PACKAGE_ROOT/flatpak-build"
REPO_DIR="$PACKAGE_ROOT/flatpak-repo"

version="$(python3 "$ROOT/scripts/read-workspace-version.py")"

if [[ ! -f "$MANIFEST" ]]; then
    printf 'Flatpak manifest not found at %s\n' "$MANIFEST" >&2
    exit 1
fi

if ! command -v flatpak >/dev/null 2>&1; then
    printf 'flatpak is required to build Flatpak artifacts\n' >&2
    exit 1
fi

flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak-builder --force-clean --repo="$REPO_DIR" "$BUILD_DIR" "$MANIFEST"

BUNDLE="$PACKAGE_ROOT/synchrosonic-${version}.flatpak"
flatpak build-bundle "$REPO_DIR" "$BUNDLE" org.synchrosonic.SynchroSonic "$version" --runtime-repo=https://flathub.org/repo/flathub.flatpakrepo

printf 'Built Flatpak bundle: %s\n' "$BUNDLE"
