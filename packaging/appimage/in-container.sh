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

# AppImageHub needs a real .DirIcon, a desktop file and metainfo in the bundle.
rm -f "$APPDIR/.DirIcon"
cp "/src/icons/hicolor/256x256/apps/$APP_ID.png" "$APPDIR/.DirIcon"
[[ -e "$APPDIR/$APP_ID.desktop" ]] || cp "$APPDIR/usr/share/applications/$APP_ID.desktop" "$APPDIR/"
desktop-file-validate "$APPDIR/usr/share/applications/$APP_ID.desktop"
appstreamcli validate --no-net "$APPDIR/usr/share/metainfo/$APP_ID.metainfo.xml"

"$TOOLS/linuxdeploy-x86_64.AppImage" --appdir "$APPDIR" --output appimage

install -m755 "$LDAI_OUTPUT" /out/DonutTaskManager-x86_64.AppImage
chown "$HOST_UID:$HOST_GID" /out/DonutTaskManager-x86_64.AppImage
