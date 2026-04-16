FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        dbus-user-session \
        flatpak \
        flatpak-builder \
        xz-utils \
        zstd \
    && rm -rf /var/lib/apt/lists/*
