#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
PKGBUILD_TEMPLATE="$ROOT/packaging/arch/PKGBUILD"
BUILD_ROOT="$ROOT/target/arch-package"

die() {
    printf '%s\n' "$1" >&2
    exit 1
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

require_command python3 "install Python 3 to read the workspace version"
require_command git "install git to stage the Arch package source tarball"
require_command tar "install tar to create the Arch package source tarball"
require_command makepkg "install pacman/makepkg on an Arch Linux build host"
require_file "$PKGBUILD_TEMPLATE" "Arch PKGBUILD"

version="$(python3 "$ROOT/scripts/read-workspace-version.py")"
pkgbuild_version="$(sed -n 's/^pkgver=//p' "$PKGBUILD_TEMPLATE" | head -n 1)"
if [[ -z "$pkgbuild_version" ]]; then
    die "unable to parse pkgver from $PKGBUILD_TEMPLATE"
fi

if [[ "$pkgbuild_version" != "$version" ]]; then
    die "PKGBUILD version ($pkgbuild_version) does not match workspace version ($version)"
fi

source_tarball="$BUILD_ROOT/synchrosonic-${version}.tar.gz"

rm -rf "$BUILD_ROOT"
mkdir -p "$BUILD_ROOT"
cp "$PKGBUILD_TEMPLATE" "$BUILD_ROOT/PKGBUILD"

(
    cd "$ROOT"
    git ls-files -z --cached --others --exclude-standard \
        | tar --null -czf "$source_tarball" \
            --transform "s|^|synchrosonic-${version}/|" \
            -T -
)

(
    cd "$BUILD_ROOT"
    makepkg -f
)

printf 'Built Arch package artifacts in %s\n' "$BUILD_ROOT"
find "$BUILD_ROOT" -maxdepth 1 -type f \( -name '*.pkg.tar.*' -o -name '*.src.tar.*' \) -print
