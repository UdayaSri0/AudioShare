#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
APP_ID="org.synchrosonic.SynchroSonic"
APP_BINARY="synchrosonic-app"
PACKAGE_ROOT="$ROOT/target/release-packaging"
PACKAGE_SCRIPT="$ROOT/scripts/package-linux.sh"
APPIMAGE_TOOL_URL="https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage"
TOOL_ROOT="$ROOT/target/tools"
APPIMAGE_TOOL="$TOOL_ROOT/appimagetool-x86_64.AppImage"
APPDIR_ROOT="$PACKAGE_ROOT/AppDir"
SOURCE_DESKTOP_FILE="$ROOT/packaging/linux/$APP_ID.desktop"
SOURCE_METAINFO_FILE="$ROOT/packaging/linux/$APP_ID.metainfo.xml"
STAGED_BINARY="$APPDIR_ROOT/usr/bin/$APP_BINARY"
STAGED_APPRUN="$APPDIR_ROOT/AppRun"
STAGED_DESKTOP_FILE="$APPDIR_ROOT/$APP_ID.desktop"
STAGED_DESKTOP_INSTALL="$APPDIR_ROOT/usr/share/applications/$APP_ID.desktop"
STAGED_ICON_FILE="$APPDIR_ROOT/$APP_ID.svg"
STAGED_DIRICON_FILE="$APPDIR_ROOT/.DirIcon"
STAGED_METAINFO_FILE="$APPDIR_ROOT/usr/share/metainfo/$APP_ID.metainfo.xml"
STAGED_APPDATA_FILE="$APPDIR_ROOT/usr/share/metainfo/$APP_ID.appdata.xml"
STAGED_README_FILE="$APPDIR_ROOT/usr/share/doc/synchrosonic/README.md"
STAGED_LICENSE_FILE="$APPDIR_ROOT/usr/share/doc/synchrosonic/LICENSE"

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

die() {
    printf '%s\n' "$1" >&2
    exit 1
}

require_file() {
    local path="$1"
    local description="$2"
    if [[ ! -f "$path" ]]; then
        die "missing ${description}: $path"
    fi
}

require_executable() {
    local path="$1"
    local description="$2"
    if [[ ! -x "$path" ]]; then
        die "missing executable ${description}: $path"
    fi
}

download_appimagetool() {
    mkdir -p "$TOOL_ROOT"
    if [[ -x "$APPIMAGE_TOOL" ]]; then
        return
    fi

    if ! command -v curl >/dev/null 2>&1; then
        die "curl is required to download appimagetool from $APPIMAGE_TOOL_URL"
    fi

    local tmp_tool
    tmp_tool="$(mktemp "$TOOL_ROOT/appimagetool-x86_64.XXXXXX")"
    if ! curl --fail --location --retry 3 --retry-delay 2 -o "$tmp_tool" "$APPIMAGE_TOOL_URL"; then
        rm -f "$tmp_tool"
        die "failed to download appimagetool from $APPIMAGE_TOOL_URL"
    fi

    chmod +x "$tmp_tool"
    mv "$tmp_tool" "$APPIMAGE_TOOL"
}

version="$(python3 "$ROOT/scripts/read-workspace-version.py")"
arch="$(uname -m)"
if [[ "$arch" != "x86_64" ]]; then
    die "AppImage packaging currently supports only x86_64, got: $arch"
fi

if [[ "$SKIP_BUILD" -eq 0 ]]; then
    bash "$PACKAGE_SCRIPT"
else
    bash "$PACKAGE_SCRIPT" --skip-build
fi

if [[ ! -d "$APPDIR_ROOT" ]]; then
    die "AppDir staging tree not found at $APPDIR_ROOT"
fi

require_file "$SOURCE_DESKTOP_FILE" "source desktop file"
require_file "$SOURCE_METAINFO_FILE" "source AppStream metadata"
require_executable "$STAGED_APPRUN" "AppDir AppRun launcher"
require_executable "$STAGED_BINARY" "AppDir application binary"
require_file "$STAGED_DESKTOP_FILE" "top-level AppDir desktop file"
require_file "$STAGED_DESKTOP_INSTALL" "installed desktop file"
require_file "$STAGED_ICON_FILE" "top-level AppDir icon"
require_file "$STAGED_DIRICON_FILE" "AppDir .DirIcon"
require_file "$STAGED_METAINFO_FILE" "installed AppStream metadata"
require_file "$STAGED_APPDATA_FILE" "AppDir appdata alias"
require_file "$STAGED_README_FILE" "AppDir README"
require_file "$STAGED_LICENSE_FILE" "AppDir LICENSE"

download_appimagetool

if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate "$SOURCE_DESKTOP_FILE" "$STAGED_DESKTOP_FILE" "$STAGED_DESKTOP_INSTALL"
else
    printf 'warning: desktop-file-validate not found; skipping desktop entry validation\n' >&2
fi

if command -v appstreamcli >/dev/null 2>&1; then
    appstreamcli validate --no-net "$SOURCE_METAINFO_FILE"
    appstreamcli validate --no-net "$STAGED_METAINFO_FILE"
    appstreamcli validate --no-net "$STAGED_APPDATA_FILE"
else
    printf 'warning: appstreamcli not found; skipping AppStream validation\n' >&2
fi

output="$PACKAGE_ROOT/synchrosonic-${version}-x86_64.AppImage"
rm -f "$output"
"$APPIMAGE_TOOL" "$APPDIR_ROOT" "$output"
chmod +x "$output"
require_executable "$output" "generated AppImage"

printf 'Built AppImage: %s\n' "$output"
