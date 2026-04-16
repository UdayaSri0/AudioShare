#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
PACKAGE_ROOT="$ROOT/target/release-packaging"
REPO_ROOT="$PACKAGE_ROOT/apt-repo"
REPOSITORY_URL="${SYNCHROSONIC_APT_REPOSITORY_URL:-https://udayasri0.github.io/AudioShare}"
DISTRIBUTION="stable"
COMPONENT="main"
SIGN_REPO=0

for arg in "$@"; do
    case "$arg" in
        --sign)
            SIGN_REPO=1
            ;;
        --repository-url=*)
            REPOSITORY_URL="${arg#*=}"
            ;;
        --distribution=*)
            DISTRIBUTION="${arg#*=}"
            ;;
        --component=*)
            COMPONENT="${arg#*=}"
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

note() {
    printf '%s\n' "$1"
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

resolve_source_date_epoch() {
    if [[ -n "${SOURCE_DATE_EPOCH:-}" ]]; then
        printf '%s\n' "$SOURCE_DATE_EPOCH"
        return
    fi

    if command -v git >/dev/null 2>&1 && git -C "$ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        local git_epoch
        git_epoch="$(git -C "$ROOT" log -1 --format=%ct 2>/dev/null || true)"
        if [[ -n "$git_epoch" ]]; then
            printf '%s\n' "$git_epoch"
            return
        fi
    fi

    date +%s
}

release_date_rfc2822() {
    date -u -R -d "@$SOURCE_DATE_EPOCH_VALUE"
}

hash_file() {
    local command_name="$1"
    local path="$2"
    "$command_name" "$path" | awk '{print $1}'
}

write_release_file() {
    local release_file="$1"
    local packages_rel="$2"
    local packages_gz_rel="$3"
    local packages_path="$4"
    local packages_gz_path="$5"
    local packages_md5
    local packages_gz_md5
    local packages_sha256
    local packages_gz_sha256
    local packages_size
    local packages_gz_size

    packages_md5="$(hash_file md5sum "$packages_path")"
    packages_gz_md5="$(hash_file md5sum "$packages_gz_path")"
    packages_sha256="$(hash_file sha256sum "$packages_path")"
    packages_gz_sha256="$(hash_file sha256sum "$packages_gz_path")"
    packages_size="$(stat -c '%s' "$packages_path")"
    packages_gz_size="$(stat -c '%s' "$packages_gz_path")"

    cat >"$release_file" <<EOF
Origin: SynchroSonic
Label: SynchroSonic
Suite: $DISTRIBUTION
Codename: $DISTRIBUTION
Version: $version
Architectures: $arch
Components: $COMPONENT
Description: $(if [[ "$SIGN_REPO" -eq 1 ]]; then printf 'Signed'; else printf 'Unsigned'; fi) GitHub Pages APT repository for SynchroSonic
Date: $(release_date_rfc2822)
MD5Sum:
 $packages_md5 $packages_size $packages_rel
 $packages_gz_md5 $packages_gz_size $packages_gz_rel
SHA256:
 $packages_sha256 $packages_size $packages_rel
 $packages_gz_sha256 $packages_gz_size $packages_gz_rel
EOF
}

write_repo_index() {
    local index_file="$1"
    local status_text
    local install_block

    if [[ "$SIGN_REPO" -eq 1 ]]; then
        status_text="Signed GitHub Pages APT repository"
        install_block=$(cat <<EOF
<p>Install the published key and source descriptor:</p>
<pre><code>curl -fsSL ${REPOSITORY_URL}/keyrings/synchrosonic-archive-keyring.gpg | sudo tee /usr/share/keyrings/synchrosonic-archive-keyring.gpg >/dev/null
curl -fsSL ${REPOSITORY_URL}/install/synchrosonic.sources | sudo tee /etc/apt/sources.list.d/synchrosonic.sources >/dev/null
sudo apt update
sudo apt install synchrosonic</code></pre>
EOF
)
    else
        status_text="Unsigned preview scaffold"
        install_block='<p>This scaffold is intentionally unsigned. Treat it as an inspectable local preview until a signing key is configured.</p>'
    fi

    cat >"$index_file" <<EOF
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>SynchroSonic APT Repository</title>
</head>
<body>
  <h1>SynchroSonic APT Repository</h1>
  <p>${status_text}</p>
  <ul>
    <li><a href="dists/${DISTRIBUTION}/Release">Release metadata</a></li>
    <li><a href="pool/main/s/synchrosonic/">Package pool</a></li>
    <li><a href="README.txt">Repository notes</a></li>
  </ul>
  ${install_block}
</body>
</html>
EOF
}

write_repo_readme() {
    local readme_file="$1"
    cat >"$readme_file" <<EOF
SynchroSonic APT repository
===========================

Status: $(if [[ "$SIGN_REPO" -eq 1 ]]; then printf 'signed'; else printf 'unsigned preview'; fi)
Repository URL: $REPOSITORY_URL
Distribution: $DISTRIBUTION
Component: $COMPONENT
Architecture: $arch
Package: $(basename "$deb_path")

Contents:
- pool/ with the Debian package
- dists/$DISTRIBUTION/$COMPONENT/binary-$arch/Packages and Packages.gz
- dists/$DISTRIBUTION/Release
EOF

    if [[ "$SIGN_REPO" -eq 1 ]]; then
        cat >>"$readme_file" <<EOF
- dists/$DISTRIBUTION/InRelease
- dists/$DISTRIBUTION/Release.gpg
- keyrings/synchrosonic-archive-keyring.gpg
- install/synchrosonic.sources
EOF
    else
        cat >>"$readme_file" <<EOF

This repository is unsigned. Do not publish it for unattended production use
until a signing key is configured and Release metadata is signed.
EOF
    fi
}

write_sources_descriptor() {
    local output_path="$1"
    cat >"$output_path" <<EOF
Types: deb
URIs: $REPOSITORY_URL
Suites: $DISTRIBUTION
Components: $COMPONENT
Architectures: $arch
Signed-By: /usr/share/keyrings/synchrosonic-archive-keyring.gpg
EOF
}

sign_release_metadata() {
    local release_file="$1"
    local release_gpg="$2"
    local inrelease="$3"
    local keyring_dir="$4"
    local install_dir="$5"
    local key_id="${APT_GPG_KEY_ID:-}"
    local passphrase="${APT_GPG_PASSPHRASE:-}"

    require_command gpg "install GnuPG before signing the APT repository"

    if [[ -z "$key_id" ]]; then
        die "APT_GPG_KEY_ID must be set when --sign is used"
    fi

    if [[ -z "$passphrase" ]]; then
        die "APT_GPG_PASSPHRASE must be set when --sign is used"
    fi

    mkdir -p "$keyring_dir" "$install_dir"

    gpg --batch --yes --pinentry-mode loopback --passphrase "$passphrase" \
        --local-user "$key_id" --output "$release_gpg" --detach-sign "$release_file"
    gpg --batch --yes --pinentry-mode loopback --passphrase "$passphrase" \
        --local-user "$key_id" --output "$inrelease" --clearsign "$release_file"

    gpg --batch --yes --export "$key_id" >"$keyring_dir/synchrosonic-archive-keyring.gpg"
    gpg --batch --yes --armor --export "$key_id" >"$keyring_dir/synchrosonic-archive-keyring.asc"
    write_sources_descriptor "$install_dir/synchrosonic.sources"
}

require_command python3 "install Python 3 to read the workspace version"
require_command dpkg "install dpkg before building the APT repository scaffold"
require_command dpkg-scanpackages "install dpkg-dev before building the APT repository scaffold"
require_command gzip "install gzip before building the APT repository scaffold"
require_command md5sum "install coreutils before building the APT repository scaffold"
require_command sha256sum "install coreutils before building the APT repository scaffold"
require_command stat "install coreutils before building the APT repository scaffold"

version="$(python3 "$ROOT/scripts/read-workspace-version.py")"
arch="$(dpkg --print-architecture)"
SOURCE_DATE_EPOCH_VALUE="$(resolve_source_date_epoch)"
deb_path="$PACKAGE_ROOT/synchrosonic_${version}_${arch}.deb"
pool_dir="$REPO_ROOT/pool/main/s/synchrosonic"
binary_dir="$REPO_ROOT/dists/$DISTRIBUTION/$COMPONENT/binary-${arch}"
release_file="$REPO_ROOT/dists/$DISTRIBUTION/Release"
release_gpg="$REPO_ROOT/dists/$DISTRIBUTION/Release.gpg"
inrelease_file="$REPO_ROOT/dists/$DISTRIBUTION/InRelease"

require_file "$deb_path" "built Debian package"

rm -rf "$REPO_ROOT"
mkdir -p "$pool_dir" "$binary_dir"
cp "$deb_path" "$pool_dir/"

pushd "$REPO_ROOT" >/dev/null
dpkg-scanpackages --arch "$arch" pool > "dists/$DISTRIBUTION/$COMPONENT/binary-${arch}/Packages"
gzip -9n -c "dists/$DISTRIBUTION/$COMPONENT/binary-${arch}/Packages" > "dists/$DISTRIBUTION/$COMPONENT/binary-${arch}/Packages.gz"
popd >/dev/null

packages_rel="$COMPONENT/binary-${arch}/Packages"
packages_gz_rel="${packages_rel}.gz"
packages_path="$REPO_ROOT/dists/$DISTRIBUTION/${packages_rel}"
packages_gz_path="$REPO_ROOT/dists/$DISTRIBUTION/${packages_gz_rel}"

write_release_file "$release_file" "$packages_rel" "$packages_gz_rel" "$packages_path" "$packages_gz_path"
write_repo_index "$REPO_ROOT/index.html"
write_repo_readme "$REPO_ROOT/README.txt"
touch "$REPO_ROOT/.nojekyll"

if [[ "$SIGN_REPO" -eq 1 ]]; then
    sign_release_metadata "$release_file" "$release_gpg" "$inrelease_file" "$REPO_ROOT/keyrings" "$REPO_ROOT/install"
    note "Built signed APT repository tree in $REPO_ROOT"
else
    note "Built unsigned APT repository scaffold in $REPO_ROOT"
fi
