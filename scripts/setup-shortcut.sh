#!/bin/bash
# Setup Ctrl+Shift+Esc global shortcut for DonutTaskManager on KDE Plasma.
# Run after building: ./scripts/setup-shortcut.sh

set -euo pipefail

APP_ID="online.donutsdelivery.DonutTaskManager"
SCRIPT_DIR="$(dirname "$(realpath "$0")")"

# Works from a source checkout (files under data/, icons/, target/release/)
# and from the release tarball (everything next to this script).
pick() {
    for candidate in "$@"; do
        if [ -f "$candidate" ]; then
            echo "$candidate"
            return
        fi
    done
    echo "$1"
}

BINARY_SRC="$(pick "$SCRIPT_DIR/../target/release/donut-taskmgr" "$SCRIPT_DIR/donut-taskmgr")"
BINARY_DST="$HOME/.local/bin/donut-taskmgr"
DESKTOP_SRC="$(pick "$SCRIPT_DIR/../data/$APP_ID.desktop" "$SCRIPT_DIR/$APP_ID.desktop")"
DESKTOP_DST="$HOME/.local/share/applications/$APP_ID.desktop"
ICON_SRC="$(pick "$SCRIPT_DIR/../icons/$APP_ID.svg" "$SCRIPT_DIR/$APP_ID.svg")"
ICON_DST="$HOME/.local/share/icons/hicolor/scalable/apps/$APP_ID.svg"

# Ensure binary exists
if [ ! -f "$BINARY_SRC" ]; then
    echo "Error: Binary not found at $BINARY_SRC"
    echo "Run 'cargo build --release' first."
    exit 1
fi

# Copy binary
mkdir -p "$HOME/.local/bin"
cp "$BINARY_SRC" "$BINARY_DST"
chmod +x "$BINARY_DST"
echo "Installed binary to $BINARY_DST"

# Install desktop file and icon
mkdir -p "$HOME/.local/share/applications" "$(dirname "$ICON_DST")"
cp "$DESKTOP_SRC" "$DESKTOP_DST"
cp "$ICON_SRC" "$ICON_DST"
echo "Installed desktop file to $DESKTOP_DST"

# Pick the KDE config tool: Plasma 6 has kwriteconfig6, Plasma 5 kwriteconfig5
KWRITECONFIG=""
for tool in kwriteconfig6 kwriteconfig5; do
    if command -v "$tool" &>/dev/null; then
        KWRITECONFIG="$tool"
        break
    fi
done

if [ -n "$KWRITECONFIG" ]; then
    # Drop the binding and files an older build (task-manager-linux) registered
    "$KWRITECONFIG" --file kglobalshortcutsrc \
        --group "services" --group "task-manager.desktop" \
        --key "_launch" --delete
    rm -f "$HOME/.local/share/applications/task-manager.desktop" \
        "$HOME/.local/bin/task-manager-linux"

    "$KWRITECONFIG" --file kglobalshortcutsrc \
        --group "services" --group "$APP_ID.desktop" \
        --key "_launch" "Ctrl+Shift+Esc,none,DonutTaskManager"

    echo "Registered Ctrl+Shift+Esc shortcut in KDE"
    echo "Log out and back in for the shortcut to take effect."
else
    echo "Note: kwriteconfig6/kwriteconfig5 not found. Set Ctrl+Shift+Esc manually in System Settings > Shortcuts."
fi

echo "Done! Press Ctrl+Shift+Esc to open DonutTaskManager."
