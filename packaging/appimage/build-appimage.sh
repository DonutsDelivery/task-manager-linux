#!/usr/bin/env bash
# Build DonutTaskManager-x86_64.AppImage in an Ubuntu 24.04 container, so the
# bundle only needs glibc 2.39 or newer on the host.
#
# Usage: packaging/appimage/build-appimage.sh
# Output: dist/appimage/DonutTaskManager-x86_64.AppImage
#
# Wrap the call in host-heavy-build on the DonutStudio machine, since it runs a
# full cargo release build.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
CACHE="${DONUTTASKMGR_CACHE:-$HOME/.cache/donuttaskmanager-appimage}"
OUT="${OUT_DIR:-$ROOT/dist/appimage}"
IMAGE="${BASE_IMAGE:-ubuntu:24.04}"

mkdir -p "$CACHE" "$OUT"

docker run --rm \
    -v "$ROOT:/src:ro" \
    -v "$CACHE:/cache" \
    -v "$OUT:/out" \
    -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
    -e RUST_VERSION="${RUST_VERSION:-1.99.1}" \
    "$IMAGE" bash /src/packaging/appimage/in-container.sh

sha256sum "$OUT/DonutTaskManager-x86_64.AppImage"
