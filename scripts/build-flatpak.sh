#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
PACKAGE_ROOT="$ROOT/target/release-packaging"
RUNNER_SCRIPT="$ROOT/scripts/build-flatpak-runner.sh"
MANIFEST="$ROOT/packaging/flatpak/org.synchrosonic.SynchroSonic.yml"
DOCKERFILE="$ROOT/packaging/flatpak/Dockerfile.builder"
LOCAL_DOCKER_IMAGE="synchrosonic-flatpak-builder:24.04"
DOCKER_IMAGE="${SYNCHROSONIC_FLATPAK_DOCKER_IMAGE:-$LOCAL_DOCKER_IMAGE}"
FLATPAK_HOME_ROOT="$ROOT/target/flatpak-home"
REBUILD_LOCAL_DOCKER_IMAGE="${SYNCHROSONIC_FLATPAK_REBUILD_DOCKER_IMAGE:-0}"
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

require_command() {
    local command_name="$1"
    local install_hint="$2"
    if ! command -v "$command_name" >/dev/null 2>&1; then
        printf '%s\n' "${command_name} is required; ${install_hint}" >&2
        exit 1
    fi
}

require_command python3 "install Python 3 to read the workspace version"
version="$(python3 "$ROOT/scripts/read-workspace-version.py")"
BUNDLE="$PACKAGE_ROOT/synchrosonic-${version}.flatpak"
export SYNCHROSONIC_WORKSPACE_VERSION="$version"

die() {
    printf '%s\n' "$1" >&2
    exit 1
}

note() {
    printf '%s\n' "$1"
}

require_file() {
    local path="$1"
    local description="$2"
    if [[ ! -f "$path" ]]; then
        die "missing ${description}: $path"
    fi
}

require_file "$MANIFEST" "Flatpak manifest"
require_file "$RUNNER_SCRIPT" "Flatpak runner script"

if [[ "$SKIP_BUILD" -eq 0 ]]; then
    cargo build --release -p synchrosonic-app
fi

export SYNCHROSONIC_FLATPAK_HOME="$FLATPAK_HOME_ROOT/home"
export SYNCHROSONIC_FLATPAK_CACHE_HOME="$FLATPAK_HOME_ROOT/cache"
export SYNCHROSONIC_FLATPAK_DATA_HOME="$FLATPAK_HOME_ROOT/data"

mkdir -p "$PACKAGE_ROOT" "$SYNCHROSONIC_FLATPAK_HOME" "$SYNCHROSONIC_FLATPAK_CACHE_HOME" "$SYNCHROSONIC_FLATPAK_DATA_HOME"

native_tooling_available() {
    command -v flatpak >/dev/null 2>&1 && command -v flatpak-builder >/dev/null 2>&1
}

run_native_build() {
    note "Using native Flatpak tooling."
    bash "$RUNNER_SCRIPT"
}

docker_available() {
    command -v docker >/dev/null 2>&1
}

docker_reachable() {
    docker info >/dev/null 2>&1
}

ensure_local_docker_image() {
    require_file "$DOCKERFILE" "Flatpak Dockerfile fallback"
    if docker image inspect "$LOCAL_DOCKER_IMAGE" >/dev/null 2>&1 && [[ "$REBUILD_LOCAL_DOCKER_IMAGE" != "1" ]]; then
        note "Reusing cached Flatpak Docker fallback image: $LOCAL_DOCKER_IMAGE"
        return
    fi

    note "Building local Flatpak Docker fallback image: $LOCAL_DOCKER_IMAGE"
    DOCKER_BUILDKIT=0 docker build --network host -t "$LOCAL_DOCKER_IMAGE" -f "$DOCKERFILE" "$ROOT/packaging/flatpak"
}

run_docker_image() {
    local image="$1"
    docker run --rm --privileged \
        --network host \
        --user "$(id -u):$(id -g)" \
        -e SYNCHROSONIC_FLATPAK_HOME \
        -e SYNCHROSONIC_FLATPAK_CACHE_HOME \
        -e SYNCHROSONIC_FLATPAK_DATA_HOME \
        -e SYNCHROSONIC_WORKSPACE_VERSION \
        -v "$ROOT:$ROOT" \
        -w "$ROOT" \
        "$image" \
        bash "$RUNNER_SCRIPT"
}

run_docker_build() {
    if ! docker_available; then
        die "native Flatpak tooling is unavailable and docker is not installed for the fallback path"
    fi

    if ! docker_reachable; then
        die "native Flatpak tooling is unavailable and docker is not reachable for the fallback path"
    fi

    if [[ "$DOCKER_IMAGE" != "$LOCAL_DOCKER_IMAGE" ]]; then
        note "Trying configured Flatpak Docker image: $DOCKER_IMAGE"
        if run_docker_image "$DOCKER_IMAGE"; then
            return 0
        fi

        note "Configured Flatpak builder image failed; falling back to the repository Dockerfile image."
    fi

    ensure_local_docker_image
    note "Using Docker-based Flatpak fallback tooling."
    run_docker_image "$LOCAL_DOCKER_IMAGE"
}

if native_tooling_available; then
    run_native_build
else
    note "Native Flatpak tooling not found; trying the Docker-based Flatpak builder."
    run_docker_build
fi

require_file "$BUNDLE" "Flatpak bundle"

printf 'Built Flatpak bundle: %s\n' "$BUNDLE"
