#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
PACKAGE_ROOT="$ROOT/target/release-packaging"
REQUIRE_RPM=0

for arg in "$@"; do
    case "$arg" in
        --require-rpm)
            REQUIRE_RPM=1
            ;;
        *)
            printf 'unknown argument: %s\n' "$arg" >&2
            exit 2
            ;;
    esac
done

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
manifests=("$PACKAGE_ROOT"/RELEASE_MANIFEST.json)
rpms=("$PACKAGE_ROOT"/*.rpm)

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

if [[ "${#manifests[@]}" -eq 0 ]]; then
    printf 'missing RELEASE_MANIFEST.json in %s\n' "$PACKAGE_ROOT" >&2
    missing=1
fi

if [[ "$REQUIRE_RPM" -eq 1 && "${#rpms[@]}" -eq 0 ]]; then
    printf 'missing RPM package artifact in %s\n' "$PACKAGE_ROOT" >&2
    missing=1
fi

if [[ "$missing" -ne 0 ]]; then
    exit 1
fi

if [[ "$REQUIRE_RPM" -eq 1 ]]; then
    printf 'RPM verification enabled; found %s RPM artifact(s).\n' "${#rpms[@]}"
fi

printf 'Release artifact verification passed.\n'
