#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
APP_ID="org.synchrosonic.SynchroSonic"
APP_BINARY="synchrosonic-app"
PACKAGE_ROOT="$ROOT/target/release-packaging"
PACKAGE_SCRIPT="$ROOT/scripts/package-linux.sh"
APPIMAGE_TOOL_URL="https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage"
APPIMAGE_TOOL="$PACKAGE_ROOT/tools/appimagetool-x86_64.AppImage"

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

version="$(cargo pkgid -p synchrosonic-app)"
version="${version##*#}"
arch="$(uname -m)"
if [[ "$arch" != "x86_64" ]]; then
    printf 'unsupported architecture: %s\n' "$arch" >&2
    exit 1
fi

if [[ "$SKIP_BUILD" -eq 0 ]]; then
    bash "$PACKAGE_SCRIPT"
else
    bash "$PACKAGE_SCRIPT" --skip-build
fi

if [[ ! -d "$PACKAGE_ROOT/AppDir" ]]; then
    printf 'AppDir staging tree not found at %s\n' "$PACKAGE_ROOT/AppDir" >&2
    exit 1
fi

mkdir -p "$PACKAGE_ROOT/tools"
if [[ ! -x "$APPIMAGE_TOOL" ]]; then
    curl -L -o "$APPIMAGE_TOOL" "$APPIMAGE_TOOL_URL"
    chmod +x "$APPIMAGE_TOOL"
fi

DESKTOP_FILE="$ROOT/packaging/linux/$APP_ID.desktop"
METAINFO_FILE="$ROOT/packaging/linux/$APP_ID.metainfo.xml"

if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate "$DESKTOP_FILE"
fi

if command -v appstreamcli >/dev/null 2>&1; then
    appstreamcli validate --no-net "$METAINFO_FILE"
fi

output="$PACKAGE_ROOT/synchrosonic-${version}-x86_64.AppImage"
"$APPIMAGE_TOOL" "$PACKAGE_ROOT/AppDir" "$output"
chmod +x "$output"

printf 'Built AppImage: %s\n' "$output"
