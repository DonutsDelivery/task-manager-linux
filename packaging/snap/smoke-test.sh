#!/usr/bin/env bash
# Check the built snap's payload without snapd. An Ubuntu 24.04 container with
# GTK 4.14 and libadwaita 1.5 stands in for the gnome-46-2404 content snap. The
# app starts on a private Xvfb and the script saves a screenshot. Confinement and
# interface connections need a real snapd and are not covered here.
#
# Usage: packaging/snap/smoke-test.sh [snap-file] [output-dir]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SNAP_FILE="$(readlink -f "${1:-$(ls "$HERE"/../../dist/snap/*.snap | head -1)}")"
OUTDIR="${2:-$HERE/../../dist/snap}"
mkdir -p "$OUTDIR"
OUTDIR="$(readlink -f "$OUTDIR")"
chmod a+rwx "$OUTDIR"   # the container user is not the host user

docker run --rm \
    -v "$SNAP_FILE:/app.snap:ro" \
    -v "$OUTDIR:/out" \
    ubuntu:24.04 bash -euo pipefail -c '
        export DEBIAN_FRONTEND=noninteractive
        apt-get update -qq
        apt-get install -y -qq --no-install-recommends squashfs-tools xvfb xauth x11-utils \
            imagemagick fonts-dejavu-core libgtk-4-1 libadwaita-1-0 libgl1-mesa-dri \
            adwaita-icon-theme shared-mime-info librsvg2-common dbus-x11 >/dev/null
        unsquashfs -q -d /snap-root /app.snap
        useradd -m tester
        su tester -c "
            Xvfb :90 -screen 0 1280x800x24 >/dev/null 2>&1 &
            sleep 2
            export DISPLAY=:90 GSK_RENDERER=cairo LIBGL_ALWAYS_SOFTWARE=1 NO_AT_BRIDGE=1
            ldd /snap-root/bin/donut-taskmgr | grep \"not found\" && exit 1
            dbus-run-session -- /snap-root/bin/donut-taskmgr >~/app.log 2>&1 &
            for i in \$(seq 1 30); do
                xwininfo -root -tree | grep -q DonutTaskManager && break
                sleep 1
            done
            sleep 6
            xwininfo -root -tree | grep DonutTaskManager
            import -window root /out/snap-smoke.png
            head -20 ~/app.log
        "
    '
echo "Screenshot: $OUTDIR/snap-smoke.png"
