#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
PACKAGE_ROOT="$ROOT/target/release-packaging"

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

if [[ "$SKIP_BUILD" -eq 0 ]]; then
    cargo build --release -p synchrosonic-app
fi

bash "$ROOT/scripts/build-appimage.sh" --skip-build
bash "$ROOT/scripts/build-deb.sh" --skip-build
bash "$ROOT/scripts/build-flatpak.sh"

version="$(cargo pkgid -p synchrosonic-app)"
version="${version##*#}"
arch="$(uname -m)"

if [[ "$arch" != "x86_64" ]]; then
    printf 'unsupported architecture: %s\n' "$arch" >&2
    exit 1
fi

artifacts=(
    "$PACKAGE_ROOT/synchrosonic-${version}-x86_64.AppImage"
    "$PACKAGE_ROOT/synchrosonic_${version}_amd64.deb"
    "$PACKAGE_ROOT/synchrosonic-${version}.flatpak"
    "$PACKAGE_ROOT/synchrosonic-${version}-linux-${arch}.tar.gz"
)

for artifact in "${artifacts[@]}"; do
    if [[ ! -f "$artifact" ]]; then
        printf 'missing release artifact: %s\n' "$artifact" >&2
        exit 1
    fi
done

cd "$PACKAGE_ROOT"
sha256sum "${artifacts[@]}" > SHA256SUMS.txt

printf 'Generated checksum manifest: %s/SHA256SUMS.txt\n' "$PACKAGE_ROOT"
