# rwm (Rust Window Manager)

**rwm** is a dynamic, tiling window manager written in **Rust**. It interacts directly with the X Window System using the X11 protocol via the `x11rb` library.

This project was built to demonstrate low-level systems programming concepts, safe memory management in an unsafe environment (X11), and event-driven architecture. It communicates directly with the X server without relying on high-level windowing abstractions or frameworks.

![Screenshot of rwm](rwm.png)

## 🚀 Features

* **Dynamic Tiling:** Automatically arranges windows to maximize screen real estate.
* **Focus Follows Mouse:** Window focus changes instantly as you hover the mouse over windows.
* **Interactive Status Bar:** Clickable workspace indicators to switch tags, with `Mod + B` toggle visibility.
* **Multiple Layouts:** Supports **Master/Stack**, **Vertical Stack**, **Dwindle** and **Monocle** layouts.
* **Workspaces:** Supports 9 virtual desktops with independent window management.
* **Rust-Safe Interaction:** Uses `x11rb` for safe, Rust-idiomatic wrappers around the XCB library.
* **Event-Driven Architecture:** Implements a custom event loop to handle `MapRequest`, `DestroyNotify`, and `KeyPress` events efficiently.
* **Smart Focus:** Tracks active window state, handles focus passing upon window destruction, and manages input focus via XCB (even on empty workspaces).

## 🛠️ Architecture

The codebase is modularized to separate concerns, demonstrating clean software engineering principles:

* **`src/main.rs`**: The entry point. It establishes the X11 connection, detects the environment (Wayland vs X11) to auto-configure the Mod key, sets up global key bindings, and runs the primary event loop.
* **`src/state.rs`**: Manages the global state. It maintains the list of workspaces, focused windows, and handles the logic for mapping new windows and transferring focus.
* **`src/layout.rs`**: Contains pure functional logic for calculating window geometry (Vertical Stack, Master/Stack, Monocle, Dwindle).
* **`src/workspace.rs`**: data structures for managing individual workspace state.
* **`src/bar.rs`**: Handles the rendering of the top status bar using pure X11 drawing primitives (`poly_fill_rectangle`).

## ⌨️ Controls

**Mod Key:** Auto-detected.
* **Wayland/Xephyr:** `Alt` (to avoid conflict with host)
* **Native X11:** `Super` (Windows Key)

| Keybinding | Action |
| :--- | :--- |
| **Mod + Enter** | Spawn Terminal (`kitty`) |
| **Mod + P** | Run Launcher (`dmenu`) |
| **Mod + Shift + Enter** | Promote focused window to Master |
| **Mod + Space** | Cycle Layout (Master/Stack -> Vertical -> Monocle) |
| **Mod + J / K** | Cycle Focus (Next / Previous window) |
| **Mod + Shift + J / K** | Swap Window Up/Down |
| **Mod + B** | Toggle Status Bar |
| **Mod + - / |** | Switch Split Direction in Dwindle Layout |
| **Mod + 1-9** | Switch to Workspace 1-9 |
| **Mod + Shift + 1-9** | Move active window to Workspace 1-9 |
| **Mod + Shift + Q** | Close the focused window |
| **Mod + Ctrl + Q** | Quit the Window Manager |

## 📦 Prerequisites

To build and run **rwm**, you need:

* **Rust & Cargo** (Latest stable)
* **X11 Development Libraries** (libxcb)
* **Xephyr** (For safe testing inside a nested window)
* **Kitty** (Default terminal emulator)
* **dmenu** (Application launcher)

### NixOS Setup (Recommended)
If you are on NixOS, you can enter a shell with all dependencies available:

```bash
nix-shell -p cargo xorg.libxcb xorg.xorgserver kitty dmenu
```

### Ubuntu/Debian Setup
```bash
sudo apt install build-essential libxcb-shape0-dev libxcb-xfixes0-dev xserver-xephyr kitty suckless-tools
```

## 🏃‍♂️ How to Run (Development)

The safest way to develop and test the window manager is using **Xephyr**, which runs an X server inside a window on your current desktop.

1.  **Start Xephyr (The fake screen):**
    ```bash
    Xephyr :1 -ac -screen 1280x720 &
    ```

2.  **Run rwm on that screen:**
    ```bash
    DISPLAY=:1 cargo run

### Configuration (Environment Variables)

If you are running inside a nested environment (like Xephyr on Wayland) and need to force a specific modifier key (e.g., using Alt instead of Super), you can set `RWM_MOD`:

```bash
# Force using ALT key
RWM_MOD=alt DISPLAY=:1 cargo run

# Force using SUPER key
RWM_MOD=super DISPLAY=:1 cargo run
```

## 🔮 Future Roadmap

* [x] **Layouts:** Support for Master/Stack and Monocle layouts.
* [x] **Workspaces:** Support for multiple virtual desktops.
* [ ] **Configuration:** TOML-based configuration file for custom keybinds.

## 📄 License

MIT
