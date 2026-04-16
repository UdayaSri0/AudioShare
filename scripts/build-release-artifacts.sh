#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
PACKAGE_ROOT="$ROOT/target/release-packaging"
CACHE_ROOT="$ROOT/target/release-artifacts-cache"

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

version="$(python3 "$ROOT/scripts/read-workspace-version.py")"
arch="$(uname -m)"

if [[ "$arch" != "x86_64" ]]; then
    printf 'unsupported architecture: %s\n' "$arch" >&2
    exit 1
fi

appimage_artifact="$PACKAGE_ROOT/synchrosonic-${version}-x86_64.AppImage"
cached_appimage="$CACHE_ROOT/$(basename "$appimage_artifact")"

rm -rf "$CACHE_ROOT"
mkdir -p "$CACHE_ROOT"

bash "$ROOT/scripts/build-appimage.sh" --skip-build
cp "$appimage_artifact" "$cached_appimage"

bash "$ROOT/scripts/build-deb.sh" --skip-build
cp "$cached_appimage" "$appimage_artifact"
bash "$ROOT/scripts/build-flatpak.sh"

if command -v dpkg-scanpackages >/dev/null 2>&1; then
    bash "$ROOT/scripts/build-apt-repo.sh"
else
    printf 'dpkg-scanpackages not found; skipping the unsigned APT repository scaffold.\n'
fi

artifacts=(
    "$appimage_artifact"
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

bash "$ROOT/scripts/verify-release-artifacts.sh"

printf 'Generated checksum manifest: %s/SHA256SUMS.txt\n' "$PACKAGE_ROOT"
