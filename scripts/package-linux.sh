#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
APP_ID="org.synchrosonic.SynchroSonic"
APP_NAME="SynchroSonic"
APP_BINARY="synchrosonic-app"
PACKAGE_NAME="synchrosonic"
PACKAGE_ROOT="$ROOT/target/release-packaging"
STAGING_ROOT="$PACKAGE_ROOT/staging"
RELEASE_BINARY="$ROOT/target/release/$APP_BINARY"
DESKTOP_FILE="$ROOT/packaging/linux/$APP_ID.desktop"
METAINFO_FILE="$ROOT/packaging/linux/$APP_ID.metainfo.xml"
ICON_FILE="$ROOT/packaging/linux/$APP_ID.svg"
APPRUN_FILE="$ROOT/packaging/linux/AppRun"
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

resolve_source_date_epoch() {
    if [[ -n "${SOURCE_DATE_EPOCH:-}" ]]; then
        printf '%s\n' "$SOURCE_DATE_EPOCH"
        return
    fi

    if command -v git >/dev/null 2>&1 && git -C "$ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        local git_epoch
        git_epoch="$(git -C "$ROOT" log -1 --format=%ct 2>/dev/null || true)"
        if [[ -n "$git_epoch" ]]; then
            printf '%s\n' "$git_epoch"
            return
        fi
    fi

    printf '0\n'
}

create_deterministic_tarball() {
    local source_root="$1"
    local archive_path="$2"
    local archive_root_name="$3"
    local source_parent
    local source_name

    source_parent="$(dirname "$source_root")"
    source_name="$(basename "$source_root")"

    tar \
        --sort=name \
        --format=posix \
        --mtime="@${SOURCE_DATE_EPOCH_VALUE}" \
        --owner=0 \
        --group=0 \
        --numeric-owner \
        --pax-option='exthdr.name=%d/PaxHeaders/%f,delete=atime,delete=ctime' \
        --transform="s|^${source_name}|${archive_root_name}|" \
        -cf - \
        -C "$source_parent" "$source_name" | gzip -n >"$archive_path"
}

write_portable_readme() {
    local output_path="$1"
    cat >"$output_path" <<EOF
SynchroSonic portable Linux bundle
==================================

This archive contains a relocatable Linux filesystem layout for SynchroSonic.

- Launch binary: ./usr/bin/$APP_BINARY
- Desktop file: ./usr/share/applications/$APP_ID.desktop
- AppStream metadata: ./usr/share/metainfo/$APP_ID.metainfo.xml
- Documentation: ./usr/share/doc/$PACKAGE_NAME/

Runtime notes:
- PipeWire command-line tools are still required for the current audio backend.
- Local packaging details live in ./usr/share/doc/$PACKAGE_NAME/linux-packaging.md
EOF
}

version="$(python3 "$ROOT/scripts/read-workspace-version.py")"
arch="$(uname -m)"
SOURCE_DATE_EPOCH_VALUE="$(resolve_source_date_epoch)"

case "$arch" in
    x86_64)
        deb_arch="amd64"
        ;;
    aarch64)
        deb_arch="arm64"
        ;;
    armv7l)
        deb_arch="armhf"
        ;;
    *)
        deb_arch="$arch"
        ;;
esac

if [[ "$SKIP_BUILD" -eq 0 ]]; then
    cargo build --release -p synchrosonic-app
fi

require_executable "$RELEASE_BINARY" "release binary"
require_file "$DESKTOP_FILE" "desktop file"
require_file "$METAINFO_FILE" "AppStream metadata"
require_file "$ICON_FILE" "application icon"
require_executable "$APPRUN_FILE" "AppRun launcher"
require_file "$ROOT/README.md" "README"
require_file "$ROOT/LICENSE" "LICENSE"
require_file "$ROOT/docs/linux-packaging.md" "Linux packaging documentation"

rm -rf "$PACKAGE_ROOT"
mkdir -p "$PACKAGE_ROOT" "$STAGING_ROOT"

stage_usr_layout() {
    local root="$1"
    install -Dm755 "$RELEASE_BINARY" "$root/usr/bin/$APP_BINARY"
    install -Dm644 "$DESKTOP_FILE" "$root/usr/share/applications/$APP_ID.desktop"
    install -Dm644 "$METAINFO_FILE" "$root/usr/share/metainfo/$APP_ID.metainfo.xml"
    install -Dm644 "$ICON_FILE" "$root/usr/share/icons/hicolor/scalable/apps/$APP_ID.svg"
    install -Dm644 "$ROOT/README.md" "$root/usr/share/doc/$PACKAGE_NAME/README.md"
    install -Dm644 "$ROOT/LICENSE" "$root/usr/share/doc/$PACKAGE_NAME/LICENSE"
    install -Dm644 "$ROOT/docs/linux-packaging.md" "$root/usr/share/doc/$PACKAGE_NAME/linux-packaging.md"
}

native_root="$PACKAGE_ROOT/native"
appdir_root="$PACKAGE_ROOT/AppDir"
deb_root="$PACKAGE_ROOT/deb"
artifact_prefix="synchrosonic-${version}-linux-${arch}"
portable_root="$STAGING_ROOT/$artifact_prefix"

stage_usr_layout "$native_root"
stage_usr_layout "$portable_root"
stage_usr_layout "$appdir_root"
stage_usr_layout "$deb_root"

write_portable_readme "$portable_root/README.txt"

install -Dm755 "$APPRUN_FILE" "$appdir_root/AppRun"
install -Dm644 "$DESKTOP_FILE" "$appdir_root/$APP_ID.desktop"
install -Dm644 "$ICON_FILE" "$appdir_root/$APP_ID.svg"
install -Dm644 "$ICON_FILE" "$appdir_root/.DirIcon"
install -Dm644 \
    "$METAINFO_FILE" \
    "$appdir_root/usr/share/metainfo/$APP_ID.appdata.xml"

mkdir -p "$deb_root/DEBIAN"
cat >"$deb_root/DEBIAN/control" <<EOF
Package: $PACKAGE_NAME
Version: $version
Section: sound
Priority: optional
Architecture: $deb_arch
Maintainer: UdayaSri0
Homepage: https://github.com/UdayaSri0/AudioShare
Depends: pipewire-bin
Description: Linux-first LAN audio casting and receiver control
 SynchroSonic is a GTK4/libadwaita desktop application for Linux that captures
 system audio, discovers LAN receivers, streams to local-network targets, and
 exposes receiver diagnostics and local playback routing.
EOF

if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate "$DESKTOP_FILE"
fi

if command -v appstreamcli >/dev/null 2>&1; then
    appstreamcli validate --no-net "$METAINFO_FILE"
fi

create_deterministic_tarball "$portable_root" "$PACKAGE_ROOT/${artifact_prefix}.tar.gz" "$artifact_prefix"
create_deterministic_tarball "$appdir_root" "$STAGING_ROOT/${artifact_prefix}-AppDir.tar.gz" "AppDir"
create_deterministic_tarball "$deb_root" "$STAGING_ROOT/${artifact_prefix}-deb-layout.tar.gz" "${artifact_prefix}-deb-layout"

printf 'Created packaging outputs in %s\n' "$PACKAGE_ROOT"
printf '  - %s\n' "$PACKAGE_ROOT/${artifact_prefix}.tar.gz"
printf '  - %s\n' "$STAGING_ROOT/${artifact_prefix}-AppDir.tar.gz"
printf '  - %s\n' "$STAGING_ROOT/${artifact_prefix}-deb-layout.tar.gz"
printf '\n'
printf 'Note: the AppDir and Debian layout are staging artifacts for inspection.\n'
printf 'Final AppImage, Debian package, Flatpak bundle, and checksum generation live in\n'
printf 'scripts/build-release-artifacts.sh. Signing and repository publication remain manual.\n'
