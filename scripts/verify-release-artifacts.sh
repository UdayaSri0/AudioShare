#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
PACKAGE_ROOT="$ROOT/target/release-packaging"

if [[ ! -d "$PACKAGE_ROOT" ]]; then
    printf 'release packaging directory not found: %s\n' "$PACKAGE_ROOT" >&2
    exit 1
fi

printf 'Verifying release artifacts in %s\n' "$PACKAGE_ROOT"
ls -lah "$PACKAGE_ROOT"

shopt -s nullglob
appimages=("$PACKAGE_ROOT"/*.AppImage)
debs=("$PACKAGE_ROOT"/*.deb)
flatpaks=("$PACKAGE_ROOT"/*.flatpak)
tarballs=("$PACKAGE_ROOT"/*.tar.gz)
checksums=("$PACKAGE_ROOT"/SHA256SUMS.txt)

missing=0

if [[ "${#appimages[@]}" -eq 0 ]]; then
    printf 'missing AppImage artifact in %s\n' "$PACKAGE_ROOT" >&2
    missing=1
fi

if [[ "${#debs[@]}" -eq 0 ]]; then
    printf 'missing Debian package artifact in %s\n' "$PACKAGE_ROOT" >&2
    missing=1
fi

if [[ "${#flatpaks[@]}" -eq 0 ]]; then
    printf 'missing Flatpak bundle artifact in %s\n' "$PACKAGE_ROOT" >&2
    missing=1
fi

if [[ "${#tarballs[@]}" -eq 0 ]]; then
    printf 'missing portable tarball artifact in %s\n' "$PACKAGE_ROOT" >&2
    missing=1
fi

if [[ "${#checksums[@]}" -eq 0 ]]; then
    printf 'missing SHA256SUMS manifest in %s\n' "$PACKAGE_ROOT" >&2
    missing=1
fi

if [[ "$missing" -ne 0 ]]; then
    exit 1
fi

printf 'Release artifact verification passed.\n'
