#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
PACKAGE_ROOT="$ROOT/target/release-packaging"
REPO_ROOT="$PACKAGE_ROOT/apt-repo"
version="$(python3 "$ROOT/scripts/read-workspace-version.py")"
arch="$(dpkg --print-architecture)"
deb_path="$PACKAGE_ROOT/synchrosonic_${version}_${arch}.deb"
pool_dir="$REPO_ROOT/pool/main/s/synchrosonic"
binary_dir="$REPO_ROOT/dists/stable/main/binary-${arch}"

if [[ ! -f "$deb_path" ]]; then
    printf 'expected Debian package at %s before building the APT repository scaffold\n' "$deb_path" >&2
    exit 1
fi

if ! command -v dpkg-scanpackages >/dev/null 2>&1; then
    printf 'dpkg-scanpackages is required to build the APT repository scaffold\n' >&2
    exit 1
fi

rm -rf "$REPO_ROOT"
mkdir -p "$pool_dir" "$binary_dir"
cp "$deb_path" "$pool_dir/"

pushd "$REPO_ROOT" >/dev/null
dpkg-scanpackages --arch "$arch" pool > "dists/stable/main/binary-${arch}/Packages"
gzip -9c "dists/stable/main/binary-${arch}/Packages" > "dists/stable/main/binary-${arch}/Packages.gz"
popd >/dev/null

packages_rel="main/binary-${arch}/Packages"
packages_gz_rel="${packages_rel}.gz"
packages_path="$REPO_ROOT/dists/stable/${packages_rel}"
packages_gz_path="$REPO_ROOT/dists/stable/${packages_gz_rel}"
release_file="$REPO_ROOT/dists/stable/Release"
packages_md5="$(md5sum "$packages_path" | awk '{print $1}')"
packages_gz_md5="$(md5sum "$packages_gz_path" | awk '{print $1}')"
packages_sha256="$(sha256sum "$packages_path" | awk '{print $1}')"
packages_gz_sha256="$(sha256sum "$packages_gz_path" | awk '{print $1}')"
packages_size="$(stat -c '%s' "$packages_path")"
packages_gz_size="$(stat -c '%s' "$packages_gz_path")"
release_date="$(date -Ru)"

cat >"$release_file" <<EOF
Origin: SynchroSonic
Label: SynchroSonic
Suite: stable
Codename: stable
Version: $version
Architectures: $arch
Components: main
Description: Unsigned APT repository scaffold for SynchroSonic release assets
Date: $release_date
MD5Sum:
 $packages_md5 $packages_size $packages_rel
 $packages_gz_md5 $packages_gz_size $packages_gz_rel
SHA256:
 $packages_sha256 $packages_size $packages_rel
 $packages_gz_sha256 $packages_gz_size $packages_gz_rel
EOF

printf 'Built unsigned APT repository scaffold in %s\n' "$REPO_ROOT"
