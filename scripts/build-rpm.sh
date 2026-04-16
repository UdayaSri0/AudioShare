#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
PACKAGE_NAME="synchrosonic"
PACKAGE_ROOT="$ROOT/target/release-packaging"
PACKAGE_SCRIPT="$ROOT/scripts/package-linux.sh"
SPEC_TEMPLATE="$ROOT/packaging/rpm/synchrosonic.spec.in"
RPM_STAGE_ROOT="$PACKAGE_ROOT/rpm"
RPM_TOPDIR="$RPM_STAGE_ROOT/rpmbuild"
RPM_RELEASE="1"

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
require_command rpmbuild "install the rpm-build toolchain before building RPM packages"
require_command rpm "install the rpm query tools before validating RPM packages"
require_file "$SPEC_TEMPLATE" "RPM spec template"

version="$(python3 "$ROOT/scripts/read-workspace-version.py")"
host_arch="$(uname -m)"
case "$host_arch" in
    x86_64|aarch64)
        rpm_arch="$host_arch"
        ;;
    armv7l)
        rpm_arch="armv7hl"
        ;;
    *)
        die "unsupported RPM packaging architecture: $host_arch"
        ;;
esac

if [[ "$SKIP_BUILD" -eq 0 ]]; then
    bash "$PACKAGE_SCRIPT"
else
    bash "$PACKAGE_SCRIPT" --skip-build
fi

SOURCE_ARCHIVE_BASENAME="synchrosonic-${version}-linux-${host_arch}.tar.gz"
SOURCE_ARCHIVE="$PACKAGE_ROOT/$SOURCE_ARCHIVE_BASENAME"
SPEC_FILE="$RPM_TOPDIR/SPECS/${PACKAGE_NAME}.spec"
output="$PACKAGE_ROOT/${PACKAGE_NAME}-${version}-${RPM_RELEASE}.${rpm_arch}.rpm"

require_file "$SOURCE_ARCHIVE" "staged native install tarball"

rm -rf "$RPM_STAGE_ROOT" "$output"
mkdir -p \
    "$RPM_TOPDIR/BUILD" \
    "$RPM_TOPDIR/BUILDROOT" \
    "$RPM_TOPDIR/RPMS" \
    "$RPM_TOPDIR/SOURCES" \
    "$RPM_TOPDIR/SPECS" \
    "$RPM_TOPDIR/SRPMS"

cp "$SOURCE_ARCHIVE" "$RPM_TOPDIR/SOURCES/$SOURCE_ARCHIVE_BASENAME"

sed \
    -e "s|@VERSION@|$version|g" \
    -e "s|@RELEASE@|$RPM_RELEASE|g" \
    -e "s|@SOURCE_ARCHIVE@|$SOURCE_ARCHIVE_BASENAME|g" \
    -e "s|@RPM_ARCH@|$rpm_arch|g" \
    "$SPEC_TEMPLATE" >"$SPEC_FILE"

if ! rpmbuild -bb --define "_topdir $RPM_TOPDIR" --target "$rpm_arch" "$SPEC_FILE"; then
    die "rpmbuild failed while generating the RPM package"
fi

built_rpm="$(find "$RPM_TOPDIR/RPMS" -type f -name "${PACKAGE_NAME}-${version}-${RPM_RELEASE}*.rpm" | head -n 1)"
if [[ -z "$built_rpm" ]]; then
    die "rpmbuild completed without producing an RPM under $RPM_TOPDIR/RPMS"
fi

cp "$built_rpm" "$output"

if ! rpm -qip "$output" >/dev/null; then
    die "rpm -qip failed for $output"
fi

if ! rpm -qlp "$output" >/dev/null; then
    die "rpm -qlp failed for $output"
fi

printf 'Built RPM package: %s\n' "$output"
printf 'Validated package metadata with rpm -qip and rpm -qlp.\n'
