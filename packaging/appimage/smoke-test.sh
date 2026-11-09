#!/usr/bin/env bash
# Start the AppImage on a private Xvfb inside an Ubuntu 22.04 container, the base
# the AppImageHub test uses, and save a screenshot of the window.
#
# Usage: packaging/appimage/smoke-test.sh [AppImage] [output-dir]
# Output: <output-dir>/smoke.png
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APPIMAGE="$(readlink -f "${1:-$HERE/../../dist/appimage/DonutTaskManager-x86_64.AppImage}")"
OUTDIR="${2:-$HERE/../../dist/appimage}"
mkdir -p "$OUTDIR"
OUTDIR="$(readlink -f "$OUTDIR")"

docker run --rm \
    -v "$APPIMAGE:/app.AppImage:ro" \
    -v "$OUTDIR:/out" \
    ubuntu:22.04 bash -euo pipefail -c '
        export DEBIAN_FRONTEND=noninteractive
        apt-get update -qq
        apt-get install -y -qq --no-install-recommends xvfb xauth x11-utils imagemagick \
            fonts-dejavu-core libgl1 libegl1 libgl1-mesa-dri libfontconfig1 libx11-6 \
            libxcb1 libfreetype6 libharfbuzz0b dbus-x11 >/dev/null
        useradd -m tester
        su tester -c "
            cd ~
            Xvfb :90 -screen 0 1280x800x24 >/dev/null 2>&1 &
            sleep 2
            export DISPLAY=:90 GSK_RENDERER=cairo LIBGL_ALWAYS_SOFTWARE=1 NO_AT_BRIDGE=1
            cp /app.AppImage ~/app.AppImage
            APPIMAGE_EXTRACT_AND_RUN=1 dbus-run-session -- ~/app.AppImage >~/app.log 2>&1 &
            for i in \$(seq 1 30); do
                xwininfo -root -tree | grep -q \"DonutTaskManager\" && break
                sleep 1
            done
            sleep 6
            xwininfo -root -tree | grep DonutTaskManager
            import -window root /out/smoke.png
            cat ~/app.log | head -20
        "
    '
echo "Screenshot: $OUTDIR/smoke.png"
