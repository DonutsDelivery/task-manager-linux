#!/usr/bin/env bash
# Build donuttaskmanager_<version>_amd64.snap without LXD or multipass.
#
# Snapcraft runs in --destructive-mode inside an Ubuntu 24.04 container. The
# container borrows the snapcraft snap that is already installed on the host
# (bind-mounted at /snap/snapcraft/current), so nothing is installed on the host.
#
# The gnome extension asks for the gnome-46-2404-sdk build snap, which needs
# snapd. The container has none, so the build expands the extension, drops that
# one build-snaps line and builds the result. GTK4 and libadwaita come from the
# Ubuntu 24.04 -dev packages in build-packages, which match the SDK versions.
# The published snap still gets its runtime libraries from the gnome-46-2404
# content snap.
#
# The host's snap binary is mounted too, because snapcraft calls "snap pack".
#
# Usage: packaging/snap/build-snap.sh
# Output: dist/snap/donuttaskmanager_<version>_amd64.snap
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
OUT="${OUT_DIR:-$ROOT/dist/snap}"
CACHE="${DONUTTASKMGR_CACHE:-$HOME/.cache/donuttaskmanager-snap}"
SNAPCRAFT_SNAP="$(readlink -f /var/lib/snapd/snap/snapcraft/current)"
CORE24_SNAP="$(readlink -f /var/lib/snapd/snap/core24/current)"   # the loader that snapcraft's python needs
IMAGE="${BASE_IMAGE:-ubuntu:24.04}"

[[ -d "$SNAPCRAFT_SNAP" && -d "$CORE24_SNAP" ]] || { echo "the snapcraft and core24 snaps must be installed" >&2; exit 1; }
mkdir -p "$OUT" "$CACHE"

# Tracked and new files only, so target/ and dist/ stay out of the snap source.
SRC="$CACHE/src"
# Earlier runs leave root-owned build trees behind, so remove them from a container.
[[ ! -e "$SRC" ]] || docker run --rm -v "$CACHE:/cache" "$IMAGE" rm -rf /cache/src
mkdir -p "$SRC/snap"
(cd "$ROOT" && git ls-files -co --exclude-standard -z | tar --null -T - -cf -) | tar -xf - -C "$SRC"
cp "$HERE/snapcraft.yaml" "$SRC/snap/snapcraft.yaml"
chmod -R u+rwX,go+rX "$SRC"   # snap pack refuses a world-unreadable icon

docker run --rm \
    -v "$SNAPCRAFT_SNAP:/snap/snapcraft/current:ro" \
    -v "$CORE24_SNAP:/snap/core24/current:ro" \
    -v "$(command -v snap):/usr/bin/snap:ro" \
    -v "$SRC:/build" \
    -v "$OUT:/out" \
    -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
    "$IMAGE" bash -euo pipefail -c '
        export DEBIAN_FRONTEND=noninteractive LANG=C.UTF-8 LC_ALL=C.UTF-8
        apt-get update -qq
        apt-get install -y -qq --no-install-recommends \
            build-essential git curl ca-certificates squashfs-tools sudo xz-utils \
            python3 binutils file >/dev/null
        # Snapcraft reads its settings through snapctl when it thinks it runs as a snap.
        printf "#!/bin/sh\necho {}\n" > /usr/bin/snapctl && chmod +x /usr/bin/snapctl
        # The rust plugin wants rustup on PATH; on a normal build host it comes from the rustup snap.
        export RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo
        curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none >/dev/null
        export PATH="$CARGO_HOME/bin:$PATH"
        export SNAP=/snap/snapcraft/current SNAP_NAME=snapcraft SNAP_INSTANCE_NAME=snapcraft SNAP_ARCH=amd64
        export PATH="$SNAP/libexec/snapcraft:$PATH" CRYPTOGRAPHY_OPENSSL_NO_LEGACY=1
        SC="$SNAP/bin/python $SNAP/bin/snapcraft"
        cd /build
        $SC expand-extensions > /tmp/expanded.yaml
        python3 - <<PY
import re
t = open("/tmp/expanded.yaml").read()
t = re.sub(r"\n    build-snaps:\n    - gnome-46-2404-sdk", "", t)
open("/build/snap/snapcraft.yaml", "w").write(t)
PY
        grep -q "gnome-46-2404-sdk$" snap/snapcraft.yaml && grep -n "build-snaps" snap/snapcraft.yaml && exit 1
        trap "cp -r /root/.local/state/snapcraft/log /out/snapcraft-log 2>/dev/null; chown -R $HOST_UID:$HOST_GID /out" EXIT
        $SC pack --destructive-mode
        cp /build/*.snap /out/
        chown "$HOST_UID:$HOST_GID" /out/*.snap
    '
ls -l "$OUT"/*.snap
sha256sum "$OUT"/*.snap
