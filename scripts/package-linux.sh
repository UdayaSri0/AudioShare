#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
APP_ID="org.synchrosonic.SynchroSonic"
APP_NAME="SynchroSonic"
APP_BINARY="synchrosonic-app"
PACKAGE_NAME="synchrosonic"
PACKAGE_ROOT="$ROOT/target/release-packaging"
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

pkgid="$(cargo pkgid -p synchrosonic-app)"
version="${pkgid##*#}"
arch="$(uname -m)"

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

if [[ ! -x "$RELEASE_BINARY" ]]; then
    printf 'release binary not found at %s\n' "$RELEASE_BINARY" >&2
    exit 1
fi

rm -rf "$PACKAGE_ROOT"
mkdir -p "$PACKAGE_ROOT"

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

stage_usr_layout "$native_root"
stage_usr_layout "$appdir_root"
stage_usr_layout "$deb_root"

install -Dm755 "$APPRUN_FILE" "$appdir_root/AppRun"
install -Dm644 "$DESKTOP_FILE" "$appdir_root/$APP_ID.desktop"
install -Dm644 "$ICON_FILE" "$appdir_root/$APP_ID.svg"
install -Dm644 "$ICON_FILE" "$appdir_root/.DirIcon"

mkdir -p "$deb_root/DEBIAN"
cat >"$deb_root/DEBIAN/control" <<EOF
Package: $PACKAGE_NAME
Version: $version
Section: sound
Priority: optional
Architecture: $deb_arch
Maintainer: SynchroSonic Contributors
Homepage: https://github.com/synchrosonic/synchrosonic
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

artifact_prefix="synchrosonic-${version}-${arch}"
tar -C "$native_root" -czf "$PACKAGE_ROOT/${artifact_prefix}-native-layout.tar.gz" .
tar -C "$PACKAGE_ROOT" -czf "$PACKAGE_ROOT/${artifact_prefix}-AppDir.tar.gz" AppDir
tar -C "$deb_root" -czf "$PACKAGE_ROOT/${artifact_prefix}-deb-layout.tar.gz" .

printf 'Created packaging outputs in %s\n' "$PACKAGE_ROOT"
printf '  - %s\n' "$PACKAGE_ROOT/${artifact_prefix}-native-layout.tar.gz"
printf '  - %s\n' "$PACKAGE_ROOT/${artifact_prefix}-AppDir.tar.gz"
printf '  - %s\n' "$PACKAGE_ROOT/${artifact_prefix}-deb-layout.tar.gz"
printf '\n'
printf 'Note: the AppDir and Debian layout are staging artifacts. Final AppImage generation,\n'
printf 'Debian dependency metadata, signing, and repository publication are documented but\n'
printf 'not fully automated in this repository yet.\n'
