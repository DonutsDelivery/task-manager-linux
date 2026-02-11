# Task Manager Linux

A lightweight, native system task manager for Linux built with **GTK4** and **Rust**.

[![Discord](https://img.shields.io/badge/Discord-Join%20Server-5865F2?logo=discord&logoColor=white)](https://discord.gg/ZhvPhXrdZ4)
[![Ko-fi](https://img.shields.io/badge/Ko--fi-Support%20Me-FF5E5B?logo=ko-fi&logoColor=white)](https://ko-fi.com/donutsdelivery)

![Task Manager mockup](data/mockup.svg)

## Download

| Platform | Download |
|----------|----------|
| **Linux (x86_64)** | [Tarball (.tar.gz)](https://github.com/DonutsDelivery/task-manager-linux/releases/latest/download/task-manager-linux-x86_64.tar.gz) &#124; [Binary](https://github.com/DonutsDelivery/task-manager-linux/releases/latest/download/task-manager-linux-x86_64) |

### Quick Install

```bash
# Download and extract
curl -LO https://github.com/DonutsDelivery/task-manager-linux/releases/latest/download/task-manager-linux-x86_64.tar.gz
tar xzf task-manager-linux-x86_64.tar.gz
cd task-manager-linux

# Run
./task-manager-linux
```

> **Runtime dependencies:** GTK4 and libadwaita must be installed on your system.
> Most modern Linux desktops (GNOME, KDE Plasma 6) include these by default.

## Features

- **Process Management** — View, end, force kill, and reprioritize running processes
- **App Grouping** — Processes grouped by application using X11 window titles, `.desktop` files, and `/proc/comm`
- **Performance Monitoring** — Real-time CPU, memory, disk, network, and GPU graphs
- **GPU Support** — NVIDIA GPU monitoring via NVML
- **DE Restart** — Quick-access button to restart desktop environment components (KDE Plasma, GNOME, XFCE, Cinnamon, MATE)
- **Critical Process Protection** — Warning dialogs prevent accidentally killing system-critical processes like systemd or kwin
- **Global Shortcut** — Register Ctrl+Shift+Esc from inside the app (KDE Plasma)

## Building from Source

Requires Rust and GTK4/libadwaita development libraries.

```bash
# Arch Linux
sudo pacman -S gtk4 libadwaita

# Ubuntu/Debian
sudo apt install libgtk-4-dev libadwaita-1-dev

# Build
cargo build --release
```

The binary will be at `target/release/task-manager-linux`.

## Keyboard Shortcut

To register Ctrl+Shift+Esc as a global shortcut (KDE Plasma):

1. Launch the app
2. Click the hamburger menu (top right)
3. Click **Install Ctrl+Shift+Esc Shortcut**
4. Log out and back in

Or run the setup script:

```bash
./scripts/setup-shortcut.sh
```

## Tech Stack

- **Rust** with GTK4-rs and libadwaita
- **procfs** for process and system data
- **nvml-wrapper** for NVIDIA GPU monitoring
- **x11rb** for window title resolution
- **flume** for async backend-to-UI communication

## License

MIT
