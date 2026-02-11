#!/bin/bash
# Setup Ctrl+Shift+Esc global shortcut for Task Manager on KDE Plasma.
# Run after building: ./scripts/setup-shortcut.sh

set -euo pipefail

BINARY_SRC="$(dirname "$(realpath "$0")")/../target/release/task-manager-linux"
BINARY_DST="$HOME/.local/bin/task-manager-linux"
DESKTOP_SRC="$(dirname "$(realpath "$0")")/../data/task-manager.desktop"
DESKTOP_DST="$HOME/.local/share/applications/task-manager.desktop"

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

# Install desktop file
mkdir -p "$HOME/.local/share/applications"
cp "$DESKTOP_SRC" "$DESKTOP_DST"
echo "Installed desktop file to $DESKTOP_DST"

# Register KDE global shortcut via kwriteconfig6
if command -v kwriteconfig6 &>/dev/null; then
    kwriteconfig6 --file kglobalshortcutsrc \
        --group "services" --group "task-manager.desktop" \
        --key "_launch" "Ctrl+Shift+Esc,none,Task Manager"

    echo "Registered Ctrl+Shift+Esc shortcut in KDE"
    echo "Log out and back in for the shortcut to take effect."
else
    echo "Note: kwriteconfig6 not found. Set Ctrl+Shift+Esc manually in System Settings > Shortcuts."
fi

echo "Done! Press Ctrl+Shift+Esc to open Task Manager."
