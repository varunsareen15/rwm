# rwm (Rust Window Manager)

**rwm** is a dynamic, tiling window manager written in **Rust**. It interacts directly with the X Window System using the X11 protocol via the `x11rb` library.

This project was built to demonstrate low-level systems programming concepts, safe memory management in an unsafe environment (X11), and event-driven architecture. It communicates directly with the X server without relying on high-level windowing abstractions or frameworks.


## 🚀 Features

* **Dynamic Tiling:** Automatically arranges windows in a vertical stack layout to maximize screen real estate.
* **Rust-Safe Interaction:** Uses `x11rb` for safe, Rust-idiomatic wrappers around the XCB library.
* **Event-Driven Architecture:** Implements a custom event loop to handle `MapRequest`, `DestroyNotify`, and `KeyPress` events efficiently.
* **Focus Management:** Tracks active window state, handles focus passing upon window destruction, and manages input focus via `XCB`.
* **Minimalist:** Lightweight and dependency-light.

## 🛠️ Architecture

The codebase is modularized to separate concerns, demonstrating clean software engineering principles:

* **`src/main.rs`**: The entry point. It establishes the X11 connection, grabs the root window, sets up global key bindings (using `GrabKey`), and runs the primary event loop.
* **`src/state.rs`**: Manages the global state of the window manager. It maintains the list of managed windows (`Vec<Window>`) and the currently focused window (`Option<Window>`). It handles the logic for mapping new windows and transferring focus when a window is closed.
* **`src/layout.rs`**: Contains pure functional logic for calculating window geometry. It currently implements a vertical tiling algorithm that recalculates window dimensions dynamically whenever the state changes.

## ⌨️ Controls

**Mod Key:** `Super` (Windows Key)

| Keybinding | Action |
| :--- | :--- |
| **Mod + Enter** | Spawn Terminal (`kitty`) |
| **Mod + Shift + Q** | Close the focused window |
| **Mod + Ctrl + Q** | Quit the Window Manager |

## 📦 Prerequisites

To build and run **rwm**, you need:

* **Rust & Cargo** (Latest stable)
* **X11 Development Libraries** (libxcb)
* **Xephyr** (For safe testing inside a nested window)
* **Kitty** (Default terminal emulator)

### NixOS Setup (Recommended)
If you are on NixOS, you can enter a shell with all dependencies available:

```bash
nix-shell -p cargo xorg.libxcb xorg.xorgserver kitty
```

### Ubuntu/Debian Setup
```bash
sudo apt install build-essential libxcb-shape0-dev libxcb-xfixes0-dev xserver-xephyr kitty
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
    ```

## 🔮 Future Roadmap

* [ ] **Window Switching:** Vim-like bindings (`j`, `k`) to change focus between open windows.
* [ ] **Layouts:** Support for Master/Stack and Monocle layouts.
* [ ] **Workspaces:** Support for multiple virtual desktops.
* [ ] **Configuration:** TOML-based configuration file for custom keybinds.

## 📄 License

MIT
