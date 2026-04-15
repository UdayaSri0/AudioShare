#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
APP_BINARY="synchrosonic-app"
PACKAGE_NAME="synchrosonic"
PACKAGE_ROOT="$ROOT/target/release-packaging"
PACKAGE_SCRIPT="$ROOT/scripts/package-linux.sh"

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
    bash "$PACKAGE_SCRIPT"
else
    bash "$PACKAGE_SCRIPT" --skip-build
fi

DEB_ROOT="$PACKAGE_ROOT/deb"
BINARY="$DEB_ROOT/usr/bin/$APP_BINARY"

if [[ ! -x "$BINARY" ]]; then
    printf 'release binary not found in staged Debian tree at %s\n' "$BINARY" >&2
    exit 1
fi

# Create symlink for dpkg-shlibdeps to find debian/control
mkdir -p "$DEB_ROOT/debian"
ln -sf "../DEBIAN/control" "$DEB_ROOT/debian/control"

# Calculate dependencies
cd "$DEB_ROOT"
shlibs="$(dpkg-shlibdeps -O "./usr/bin/$APP_BINARY" | sed -n 's/^shlibs:Depends=//p')"
misc="$(dpkg-shlibdeps -O "./usr/bin/$APP_BINARY" | sed -n 's/^misc:Depends=//p' || true)"
cd "$ROOT"

if [[ -n "$misc" ]]; then
    deps="$shlibs, $misc"
else
    deps="$shlibs"
fi

if [[ -z "$deps" ]]; then
    printf 'unable to calculate Debian dependencies for %s\n' "$BINARY" >&2
    exit 1
fi

cat >"$DEB_ROOT/DEBIAN/control" <<EOF
Package: $PACKAGE_NAME
Version: $version
Section: sound
Priority: optional
Architecture: $deb_arch
Maintainer: SynchroSonic Contributors <synchrosonic@users.noreply.github.com>
Homepage: https://github.com/synchrosonic/synchrosonic
Depends: $deps
Description: Linux-first LAN audio casting and receiver control
 SynchroSonic is a GTK4/libadwaita desktop application for Linux that captures
 system audio, discovers LAN receivers, streams to local-network targets, and
 provides diagnostics and local playback routing.
EOF

output="$PACKAGE_ROOT/${PACKAGE_NAME}_${version}_${deb_arch}.deb"
dpkg-deb --build "$DEB_ROOT" "$output"

dpkg-deb --info "$output" >/dev/null 2>&1

printf 'Built Debian package: %s\n' "$output"
