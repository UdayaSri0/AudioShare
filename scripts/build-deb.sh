#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
APP_ID="org.synchrosonic.SynchroSonic"
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

die() {
    printf '%s\n' "$1" >&2
    exit 1
}

require_command() {
    local command_name="$1"
    if ! command -v "$command_name" >/dev/null 2>&1; then
        die "required command not found on PATH: $command_name"
    fi
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
        die "unsupported Debian packaging architecture: $arch"
    ;;
esac

require_command dpkg-shlibdeps
require_command dpkg-gencontrol
require_command dpkg-deb

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
DESKTOP_INSTALL="$DEB_ROOT/usr/share/applications/$APP_ID.desktop"
ICON_INSTALL="$DEB_ROOT/usr/share/icons/hicolor/scalable/apps/$APP_ID.svg"
METAINFO_INSTALL="$DEB_ROOT/usr/share/metainfo/$APP_ID.metainfo.xml"
DOC_ROOT="$DEB_ROOT/usr/share/doc/$PACKAGE_NAME"
CHANGELOG_VERSION="$(sed -n '1s/^[^(]*(\([^)]*\)).*/\1/p' "$CHANGELOG_FILE")"

require_executable "$BINARY" "staged Debian binary"
require_file "$SOURCE_CONTROL" "Debian source control file"
require_file "$CHANGELOG_FILE" "Debian changelog"
require_file "$DESKTOP_INSTALL" "staged desktop file"
require_file "$ICON_INSTALL" "staged icon"
require_file "$METAINFO_INSTALL" "staged AppStream metadata"
require_file "$DOC_ROOT/README.md" "staged README"
require_file "$DOC_ROOT/LICENSE" "staged LICENSE"
require_file "$DOC_ROOT/linux-packaging.md" "staged packaging documentation"

if [[ "$(sed -n '/./{p;q;}' "$SOURCE_CONTROL")" != Source:* ]]; then
    die "debian/control must begin with a Source: stanza"
fi

if ! grep -q '^Package:' "$SOURCE_CONTROL"; then
    die "debian/control is missing a Package: stanza"
fi

if [[ -z "$CHANGELOG_VERSION" ]]; then
    die "unable to parse the package version from debian/changelog"
fi

if [[ "$CHANGELOG_VERSION" != "$version" ]]; then
    die "debian/changelog version ($CHANGELOG_VERSION) does not match workspace version ($version)"
fi

mkdir -p "$DEB_ROOT/DEBIAN" "$SUBSTVARS_DIR"
rm -f "$SUBSTVARS_FILE" "$FILES_LIST_FILE" "$DEB_ROOT/DEBIAN/control" "$ROOT/debian/files"

if ! dpkg-shlibdeps -T"$SUBSTVARS_FILE" "$BINARY"; then
    die "dpkg-shlibdeps failed while resolving shared-library dependencies for $BINARY"
fi

if ! grep -q '^misc:Depends=' "$SUBSTVARS_FILE"; then
    printf 'misc:Depends=\n' >>"$SUBSTVARS_FILE"
fi

if ! dpkg-gencontrol \
    -p"$PACKAGE_NAME" \
    -c"$SOURCE_CONTROL" \
    -l"$CHANGELOG_FILE" \
    -f"$FILES_LIST_FILE" \
    -T"$SUBSTVARS_FILE" \
    -P"$DEB_ROOT" \
    -n"$(basename "$output")" \
    -DArchitecture="$deb_arch" \
    -v"$version" >/dev/null; then
    die "dpkg-gencontrol failed while generating $DEB_ROOT/DEBIAN/control"
fi

require_file "$DEB_ROOT/DEBIAN/control" "generated binary control file"

if ! dpkg-deb --build "$DEB_ROOT" "$output"; then
    die "dpkg-deb failed while building the final package at $output"
fi

if ! dpkg-deb --info "$output" >/dev/null; then
    die "dpkg-deb --info failed for $output"
fi

if ! dpkg-deb --contents "$output" >/dev/null; then
    die "dpkg-deb --contents failed for $output"
fi

printf 'Built Debian package: %s\n' "$output"
printf 'Validated package metadata with dpkg-deb --info and dpkg-deb --contents.\n'
