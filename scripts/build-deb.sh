#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
APP_BINARY="synchrosonic-app"
PACKAGE_NAME="synchrosonic"
PACKAGE_ROOT="$ROOT/target/release-packaging"
PACKAGE_SCRIPT="$ROOT/scripts/package-linux.sh"
SOURCE_CONTROL="$ROOT/debian/control"
CHANGELOG_FILE="$ROOT/debian/changelog"

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

version="$(python3 "$ROOT/scripts/read-workspace-version.py")"
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
SUBSTVARS_DIR="$PACKAGE_ROOT/debian"
SUBSTVARS_FILE="$SUBSTVARS_DIR/substvars"
FILES_LIST_FILE="$SUBSTVARS_DIR/files"
output="$PACKAGE_ROOT/${PACKAGE_NAME}_${version}_${deb_arch}.deb"

if [[ ! -x "$BINARY" ]]; then
    printf 'release binary not found in staged Debian tree at %s\n' "$BINARY" >&2
    exit 1
fi

if [[ ! -f "$SOURCE_CONTROL" ]]; then
    printf 'expected Debian source control file at %s\n' "$SOURCE_CONTROL" >&2
    exit 1
fi

if [[ ! -f "$CHANGELOG_FILE" ]]; then
    printf 'expected Debian changelog at %s\n' "$CHANGELOG_FILE" >&2
    exit 1
fi

mkdir -p "$DEB_ROOT/DEBIAN" "$SUBSTVARS_DIR"
rm -f "$SUBSTVARS_FILE" "$FILES_LIST_FILE" "$DEB_ROOT/DEBIAN/control" "$ROOT/debian/files"

dpkg-shlibdeps -T"$SUBSTVARS_FILE" "$BINARY"
if ! grep -q '^misc:Depends=' "$SUBSTVARS_FILE"; then
    printf 'misc:Depends=\n' >>"$SUBSTVARS_FILE"
fi

dpkg-gencontrol \
    -p"$PACKAGE_NAME" \
    -c"$SOURCE_CONTROL" \
    -l"$CHANGELOG_FILE" \
    -f"$FILES_LIST_FILE" \
    -T"$SUBSTVARS_FILE" \
    -P"$DEB_ROOT" \
    -n"$(basename "$output")" \
    -DArchitecture="$deb_arch" \
    -v"$version" >/dev/null

if [[ ! -f "$DEB_ROOT/DEBIAN/control" ]]; then
    printf 'failed to generate binary package control file at %s\n' "$DEB_ROOT/DEBIAN/control" >&2
    exit 1
fi

dpkg-deb --build "$DEB_ROOT" "$output"

dpkg-deb --info "$output" >/dev/null 2>&1
dpkg-deb --contents "$output" >/dev/null 2>&1

printf 'Built Debian package: %s\n' "$output"
