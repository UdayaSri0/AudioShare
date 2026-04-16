#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
PACKAGE_ROOT="$ROOT/target/release-packaging"
MANIFEST="$ROOT/packaging/flatpak/org.synchrosonic.SynchroSonic.yml"
DOCKERFILE="$ROOT/packaging/flatpak/Dockerfile.builder"
LOCAL_DOCKER_IMAGE="synchrosonic-flatpak-builder:24.04"
DOCKER_IMAGE="${SYNCHROSONIC_FLATPAK_DOCKER_IMAGE:-$LOCAL_DOCKER_IMAGE}"
FLATPAK_HOME_ROOT="$ROOT/target/flatpak-home"

version="$(python3 "$ROOT/scripts/read-workspace-version.py")"

if [[ ! -f "$MANIFEST" ]]; then
    printf 'Flatpak manifest not found at %s\n' "$MANIFEST" >&2
    exit 1
fi

export SYNCHROSONIC_FLATPAK_HOME="$FLATPAK_HOME_ROOT/home"
export SYNCHROSONIC_FLATPAK_CACHE_HOME="$FLATPAK_HOME_ROOT/cache"
export SYNCHROSONIC_FLATPAK_DATA_HOME="$FLATPAK_HOME_ROOT/data"

run_native_build() {
    if ! command -v flatpak >/dev/null 2>&1; then
        return 1
    fi

    if ! command -v flatpak-builder >/dev/null 2>&1; then
        return 1
    fi

    bash "$ROOT/scripts/build-flatpak-runner.sh"
}

run_docker_build() {
    if ! command -v docker >/dev/null 2>&1; then
        printf 'flatpak tooling is not installed and docker is unavailable for the Flatpak fallback\n' >&2
        return 1
    fi

    if ! docker ps >/dev/null 2>&1; then
        printf 'flatpak tooling is not installed and docker is not reachable for the Flatpak fallback\n' >&2
        return 1
    fi

    if [[ "$DOCKER_IMAGE" != "$LOCAL_DOCKER_IMAGE" ]]; then
        if run_docker_image "$DOCKER_IMAGE"; then
            return 0
        fi

        printf 'Configured Flatpak builder image failed; falling back to the local Dockerfile builder.\n'
    fi

    DOCKER_BUILDKIT=0 docker build --network host -t "$LOCAL_DOCKER_IMAGE" -f "$DOCKERFILE" "$ROOT/packaging/flatpak"
    run_docker_image "$LOCAL_DOCKER_IMAGE"
}

run_docker_image() {
    local image="$1"
    docker run --rm --privileged \
        --network host \
        --user "$(id -u):$(id -g)" \
        -e SYNCHROSONIC_FLATPAK_HOME \
        -e SYNCHROSONIC_FLATPAK_CACHE_HOME \
        -e SYNCHROSONIC_FLATPAK_DATA_HOME \
        -v "$ROOT:$ROOT" \
        -w "$ROOT" \
        "$image" \
        bash "$ROOT/scripts/build-flatpak-runner.sh"
}

if ! run_native_build; then
    printf 'Native Flatpak tooling not found; falling back to the Docker-based Flatpak builder.\n'
    run_docker_build
fi

BUNDLE="$PACKAGE_ROOT/synchrosonic-${version}.flatpak"
if [[ ! -f "$BUNDLE" ]]; then
    printf 'Flatpak bundle was not generated at %s\n' "$BUNDLE" >&2
    exit 1
fi

printf 'Built Flatpak bundle: %s\n' "$BUNDLE"
