#!/usr/bin/env bash
# Runs inside the Ubuntu 24.04 container started by build-appimage.sh.
# /src is the read-only checkout, /cache keeps cargo and tool downloads between
# runs, /out receives the AppImage.
set -euo pipefail

APP_ID=online.donutsdelivery.DonutTaskManager
BIN=donut-taskmgr
export DEBIAN_FRONTEND=noninteractive LANG=C.UTF-8 LC_ALL=C.UTF-8

apt-get update -qq
apt-get install -y -qq --no-install-recommends \
    build-essential pkg-config curl ca-certificates file patchelf binutils \
    desktop-file-utils appstream xz-utils git \
    libgtk-4-dev libadwaita-1-dev libx11-dev libxcb1-dev libudev-dev \
    libglib2.0-dev-bin librsvg2-common libgdk-pixbuf2.0-bin shared-mime-info \
    adwaita-icon-theme gsettings-desktop-schemas libgl1 libegl1 >/dev/null

# Rust toolchain, pinned and cached.
export RUSTUP_HOME=/cache/rustup CARGO_HOME=/cache/cargo
export PATH="$CARGO_HOME/bin:$PATH"
if [[ ! -x "$CARGO_HOME/bin/rustup" ]]; then
    curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none
fi
rustup toolchain install "$RUST_VERSION" --profile minimal
rustup default "$RUST_VERSION"
rustc --version

# Release build. The target dir lives in /cache so reruns are incremental.
export CARGO_TARGET_DIR=/cache/target
cargo build --release --locked --manifest-path /src/Cargo.toml

# Stage the AppDir.
APPDIR=/cache/AppDir
rm -rf "$APPDIR"
install -Dm755 "$CARGO_TARGET_DIR/release/$BIN" "$APPDIR/usr/bin/$BIN"
install -Dm644 "/src/data/$APP_ID.desktop" "$APPDIR/usr/share/applications/$APP_ID.desktop"
install -Dm644 "/src/data/$APP_ID.metainfo.xml" "$APPDIR/usr/share/metainfo/$APP_ID.metainfo.xml"
install -Dm644 "/src/icons/$APP_ID.svg" "$APPDIR/usr/share/icons/hicolor/scalable/apps/$APP_ID.svg"
for size in 16 22 24 32 48 64 128 256 512; do
    install -Dm644 "/src/icons/hicolor/${size}x${size}/apps/$APP_ID.png" \
        "$APPDIR/usr/share/icons/hicolor/${size}x${size}/apps/$APP_ID.png"
done
install -Dm644 /src/LICENSE "$APPDIR/usr/share/licenses/$BIN/LICENSE"
# The sidebar and header buttons use Adwaita symbolic icons. The GTK plugin does
# not copy the theme for GTK4, and minimal desktops may not have it installed.
mkdir -p "$APPDIR/usr/share/icons"
cp -a /usr/share/icons/Adwaita "$APPDIR/usr/share/icons/Adwaita"

# GTK finds SVG icons through the shared MIME database. Without it on the host the
# symbolic icons fail to load, so ship a copy. XDG_DATA_DIRS includes the AppDir.
mkdir -p "$APPDIR/usr/share"
cp -a /usr/share/mime "$APPDIR/usr/share/mime"

# linuxdeploy and its GTK plugin.
TOOLS=/cache/tools
mkdir -p "$TOOLS"
[[ -x "$TOOLS/linuxdeploy-x86_64.AppImage" ]] || {
    curl -fsSL -o "$TOOLS/linuxdeploy-x86_64.AppImage" \
        https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage
    chmod +x "$TOOLS/linuxdeploy-x86_64.AppImage"
}
[[ -x "$TOOLS/linuxdeploy-plugin-gtk.sh" ]] || {
    curl -fsSL -o "$TOOLS/linuxdeploy-plugin-gtk.sh" \
        https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/master/linuxdeploy-plugin-gtk.sh
    chmod +x "$TOOLS/linuxdeploy-plugin-gtk.sh"
}
sha256sum "$TOOLS"/linuxdeploy-x86_64.AppImage "$TOOLS"/linuxdeploy-plugin-gtk.sh

export APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=1
export DEPLOY_GTK_VERSION=4
export PATH="$TOOLS:$PATH"
export LDAI_OUTPUT="/cache/DonutTaskManager-x86_64.AppImage"
rm -f "$LDAI_OUTPUT"

cd /cache
"$TOOLS/linuxdeploy-x86_64.AppImage" --appdir "$APPDIR" \
    --executable "$APPDIR/usr/bin/$BIN" \
    --desktop-file "$APPDIR/usr/share/applications/$APP_ID.desktop" \
    --icon-file "/src/icons/hicolor/256x256/apps/$APP_ID.png" \
    --plugin gtk

# Bundle glibc with its loader. The libraries come from Ubuntu 24.04 (glibc 2.39),
# but GTK 4.12 and libadwaita 1.4 do not exist for Ubuntu 22.04, the oldest release
# the AppImageHub test accepts as a base. A bundled C library keeps the AppImage
# independent of the host's glibc. linuxdeploy excludes all of these, so copy them.
LIBDIR=/lib/x86_64-linux-gnu
mkdir -p "$APPDIR/usr/lib"
for lib in ld-linux-x86-64.so.2 libc.so.6 libm.so.6 libmvec.so.1 libdl.so.2 libpthread.so.0 \
           librt.so.1 libutil.so.1 libresolv.so.2 libanl.so.1 libnss_files.so.2 libnss_dns.so.2 \
           libstdc++.so.6 libgcc_s.so.1; do
    cp -L "$LIBDIR/$lib" "$APPDIR/usr/lib/$lib"
done
cp -a /usr/lib/x86_64-linux-gnu/gconv "$APPDIR/usr/lib/gconv"

# linuxdeploy skips libraries on its exclude list, such as fribidi, freetype and
# harfbuzz, expecting the host to have them. A host may not, or may have versions
# too old for GTK 4.14, so copy whatever still resolves outside the AppDir. Only the
# libraries tied to the host's graphics stack and fonts stay on the host.
HOST_ONLY='^(libGL|libEGL|libGLX|libGLdispatch|libOpenGL|libgbm|libdrm|libX|libxcb|libwayland|libxkbcommon|libfontconfig|libvulkan|libudev|libsystemd|libdbus|libasound|libpulse|libselinux|libapparmor)'
bundle_missing() {
    local f dep
    while IFS= read -r -d '' f; do
        [[ "$(head -c4 "$f" | od -An -tx1 | tr -d ' ')" == 7f454c46 ]] || continue   # ELF files only
        { "$APPDIR/usr/lib/ld-linux-x86-64.so.2" --library-path "$APPDIR/usr/lib" --list "$f" 2>/dev/null || true; } \
            | awk '/=> \/(usr\/)?lib/ {print $3}' | while read -r dep; do
                [[ "$(basename "$dep")" =~ $HOST_ONLY ]] && continue
                [[ -e "$APPDIR/usr/lib/$(basename "$dep")" ]] || { echo "bundling $dep"; cp -L "$dep" "$APPDIR/usr/lib/"; }
            done
    done < <(find "$APPDIR/usr" -type f \( -name '*.so*' -o -path '*/usr/bin/*' \) -print0)
}
bundle_missing
bundle_missing

# Start through the bundled loader. The loader reads /etc/ld.so.cache for the
# libraries that stay on the host (X11, Mesa, fontconfig).
# The loader copy carries the app's name, so process lists show donut-taskmgr
# instead of ld-linux-x86-64.so.2.
install -Dm755 "$APPDIR/usr/lib/ld-linux-x86-64.so.2" "$APPDIR/usr/libexec/donut-taskmgr"
rm -f "$APPDIR/AppRun" "$APPDIR/AppRun.wrapped"
cat > "$APPDIR/AppRun" <<'APPRUN'
#! /usr/bin/env bash
set -e
export APPDIR="$(dirname "$(readlink -f "$0")")"
source "$APPDIR/apprun-hooks/linuxdeploy-plugin-gtk.sh"
export GCONV_PATH="$APPDIR/usr/lib/gconv"
PIXBUF_LOADERS="$APPDIR/usr/lib/gdk-pixbuf-2.0/2.10.0/loaders"
export GDK_PIXBUF_MODULEDIR="$PIXBUF_LOADERS"
exec "$APPDIR/usr/libexec/donut-taskmgr" \
    --library-path "$APPDIR/usr/lib:$PIXBUF_LOADERS" \
    "$APPDIR/usr/bin/donut-taskmgr" "$@"
APPRUN
chmod +x "$APPDIR/AppRun"

# AppImageHub needs a real .DirIcon, a desktop file and metainfo in the bundle.
rm -f "$APPDIR/.DirIcon"
cp "/src/icons/hicolor/256x256/apps/$APP_ID.png" "$APPDIR/.DirIcon"
[[ -e "$APPDIR/$APP_ID.desktop" ]] || cp "$APPDIR/usr/share/applications/$APP_ID.desktop" "$APPDIR/"
desktop-file-validate "$APPDIR/usr/share/applications/$APP_ID.desktop"
appstreamcli validate --no-net "$APPDIR/usr/share/metainfo/$APP_ID.metainfo.xml"

# Pack with linuxdeploy's own output plugin. Running linuxdeploy again would replace
# the AppRun above with its generated one.
rm -rf "$TOOLS/ld-root" && (cd "$TOOLS" && ./linuxdeploy-x86_64.AppImage --appimage-extract >/dev/null && mv squashfs-root ld-root)
"$TOOLS/ld-root/usr/bin/linuxdeploy-plugin-appimage" --appdir "$APPDIR"

install -m755 "$LDAI_OUTPUT" /out/DonutTaskManager-x86_64.AppImage
chown "$HOST_UID:$HOST_GID" /out/DonutTaskManager-x86_64.AppImage
