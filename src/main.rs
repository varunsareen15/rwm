mod bar;
mod layout;
mod state;
mod workspace;

use state::WindowManager;
use std::process::Command;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{self, ConnectionExt, ModMask};

#[derive(Debug, Clone, Copy)]
enum ModKey {
    Super,
    Alt,
}

fn detect_mod_key() -> ModKey {
    use std::env;
    if let Ok(val) = env::var("RWM_MOD") {
        match val.to_lowercase().as_str() {
            "alt" => return ModKey::Alt,
            "super" | "mod4" => return ModKey::Super,
            _ => {}
        }
    }

    let session_type = env::var("XDG_SESSION_TYPE").unwrap_or_default();
    let wayland_display = env::var("WAYLAND_DISPLAY").ok();

    if session_type == "wayland" || wayland_display.is_some() {
        ModKey::Alt
    } else {
        ModKey::Super
    }
}

fn mod_mask_for(mod_key: ModKey) -> ModMask {
    match mod_key {
        ModKey::Super => ModMask::M4,
        ModKey::Alt => ModMask::M1,
    }
}
// Keysyms
const XK_RETURN: u32 = 0xff0d;
const XK_SPACE: u32 = 0x0020;
const XK_Q: u32 = 0x0071;
const XK_J: u32 = 0x006a;
const XK_K: u32 = 0x006b;
const XK_P: u32 = 0x0070;
const XK_B: u32 = 0x0062;
const XK_1: u32 = 0x0031;
const XK_9: u32 = 0x0039;
const XK_MINUS: u32 = 0x002d;
const XK_BACKSLASH: u32 = 0x005c;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let (conn, screen_num) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen_num];

    log::info!(
        "Connected. Screen: {}x{}",
        screen.width_in_pixels,
        screen.height_in_pixels
    );

    setup_cursor(&conn, screen)?;

    let change = xproto::ChangeWindowAttributesAux::new().event_mask(
        xproto::EventMask::SUBSTRUCTURE_REDIRECT | xproto::EventMask::SUBSTRUCTURE_NOTIFY,
    );

    conn.change_window_attributes(screen.root, &change)?;

    let mod_key = detect_mod_key();
    let main_mod = mod_mask_for(mod_key);
    log::info!("Using mod key: {:?}", mod_key);

    // --- Setup Keybinds ---
    // We now scan for J and K as well
    let (k_ret, k_space, k_q, k_j, k_k, k_p, k_b, k_minus, k_backslash, key_map) =
        scan_key_codes(&conn)?;

    // Grab keys
    if let Some(code) = k_ret {
        grab_key(&conn, screen.root, code, main_mod)?;
        grab_key(&conn, screen.root, code, main_mod | ModMask::SHIFT)?;
    }

    if let Some(code) = k_space {
        grab_key(&conn, screen.root, code, main_mod)?;
    }

    if let Some(code) = k_q {
        grab_key(&conn, screen.root, code, main_mod | ModMask::SHIFT)?;
        grab_key(&conn, screen.root, code, main_mod | ModMask::CONTROL)?;
    }

    if let Some(code) = k_j {
        grab_key(&conn, screen.root, code, main_mod)?;
        grab_key(&conn, screen.root, code, main_mod | ModMask::SHIFT)?;
    }
    if let Some(code) = k_k {
        grab_key(&conn, screen.root, code, main_mod)?;
        grab_key(&conn, screen.root, code, main_mod | ModMask::SHIFT)?;
    }

    if let Some(code) = k_p {
        grab_key(&conn, screen.root, code, main_mod)?;
    }

    if let Some(code) = k_b {
        grab_key(&conn, screen.root, code, main_mod)?;
    }

    if let Some(code) = k_minus {
        grab_key(&conn, screen.root, code, main_mod)?;
    }
    if let Some(code) = k_backslash {
        grab_key(&conn, screen.root, code, main_mod | ModMask::SHIFT)?;
    }

    for &(code, _) in &key_map {
        // Super + # (Switch)
        grab_key(&conn, screen.root, code, main_mod)?;
        // Super + Shift + # (Move Window to ws)
        grab_key(&conn, screen.root, code, main_mod | ModMask::SHIFT)?;
    }

    conn.flush()?;
    // ----------------------

    let mut wm_state = WindowManager::new(&conn, screen)?;
    log::info!("RWM STARTED");

    loop {
        conn.flush()?;
        let event = conn.wait_for_event()?;

        match event {
            Event::MapRequest(evt) => {
                wm_state.handle_map_request(&conn, evt.window)?;
            }
            Event::DestroyNotify(evt) => {
                wm_state.handle_destroy_notify(&conn, evt.window)?;
            }
            Event::Expose(evt) => {
                wm_state.handle_expose(&conn, evt)?;
            }
            Event::EnterNotify(evt) => {
                wm_state.handle_enter_notify(&conn, evt)?;
            }
            Event::ButtonPress(evt) => {
                if evt.event == wm_state.bar.window {
                    wm_state.handle_bar_click(&conn, evt.event_x)?;
                }
            }
            Event::KeyPress(evt) => {
                let modifiers = u16::from(evt.state);
                let key = evt.detail;

                let mod_super = u16::from(main_mod);
                let mod_shift = u16::from(ModMask::SHIFT);
                let mod_ctrl = u16::from(ModMask::CONTROL);

                if Some(key) == k_ret && (modifiers & mod_super != 0) {
                    if modifiers & mod_shift != 0 {
                        wm_state.promote_focused_to_master(&conn)?;
                    } else {
                        spawn("kitty");
                    }
                } else if Some(key) == k_q {
                    if (modifiers & mod_super != 0) && (modifiers & mod_shift != 0) {
                        wm_state.kill_focused_window(&conn)?;
                    } else if (modifiers & mod_super != 0) && (modifiers & mod_ctrl != 0) {
                        wm_state.kill_all_windows(&conn)?;
                        break;
                    }
                } else if Some(key) == k_j && (modifiers & mod_super != 0) {
                    if modifiers & mod_shift != 0 {
                        wm_state.move_focused_window(&conn, state::FocusDirection::Next)?;
                    } else {
                        wm_state.cycle_focus(&conn, state::FocusDirection::Next)?;
                    }
                } else if Some(key) == k_k && (modifiers & mod_super != 0) {
                    if modifiers & mod_shift != 0 {
                        wm_state.move_focused_window(&conn, state::FocusDirection::Prev)?;
                    } else {
                        wm_state.cycle_focus(&conn, state::FocusDirection::Prev)?;
                    }
                } else if Some(key) == k_p && (modifiers & mod_super != 0) {
                    spawn("dmenu_run");
                } else if Some(key) == k_b && (modifiers & mod_super != 0) {
                    wm_state.toggle_bar(&conn)?;
                } else if Some(key) == k_minus && (modifiers & mod_super != 0) {
                    wm_state.set_split_direction(&conn, workspace::SplitAxis::Vertical)?;
                } else if Some(key) == k_backslash
                    && (modifiers & mod_super != 0)
                    && (modifiers & mod_shift != 0)
                {
                    wm_state.set_split_direction(&conn, workspace::SplitAxis::Horizontal)?;
                } else if Some(key) == k_space && (modifiers & mod_super != 0) {
                    wm_state.cycle_layout(&conn)?;
                } else if let Some(&(_, ws_index)) = key_map.iter().find(|(code, _)| *code == key) {
                    if modifiers & mod_super != 0 {
                        if modifiers & mod_shift != 0 {
                            wm_state.move_window_to_workspace(&conn, ws_index)?;
                        } else {
                            wm_state.switch_workspace(&conn, ws_index)?;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn grab_key(
    conn: &impl Connection,
    root: xproto::Window,
    keycode: u8,
    modifiers: ModMask,
) -> Result<(), Box<dyn std::error::Error>> {
    let ignored_modifiers = [
        ModMask::default(),
        ModMask::M2,
        ModMask::LOCK,
        ModMask::M2 | ModMask::LOCK,
    ];

    for ignored in ignored_modifiers {
        conn.grab_key(
            true,
            root,
            modifiers | ignored,
            keycode,
            xproto::GrabMode::ASYNC,
            xproto::GrabMode::ASYNC,
        )?;
    }
    Ok(())
}

fn spawn(command: &str) {
    match Command::new(command)
        .env_remove("WAYLAND_DISPLAY")
        .env("XDG_SESSION_TYPE", "x11")
        .env("GDK_BACKEND", "x11")
        .env("QT_QPA_PLATFORM", "xcb")
        .env("MOZ_ENABLE_WAYLAND", "0")
        .env("ELECTRON_OZONE_PLATFORM_HINT", "auto")
        .spawn()
    {
        Ok(_) => log::info!("Spawned {}", command),
        Err(e) => log::error!("Failed to open {}: {}", command, e),
    }
}

// Updated to return 4 keys
fn scan_key_codes(
    conn: &impl Connection,
) -> Result<
    (
        Option<u8>, // Return
        Option<u8>, // Space
        Option<u8>, // J
        Option<u8>, // K
        Option<u8>, // Q
        Option<u8>, // P
        Option<u8>, // B
        Option<u8>, // Minus
        Option<u8>, // Backslash
        Vec<(u8, usize)>,
    ),
    Box<dyn std::error::Error>,
> {
    let setup = conn.setup();
    let min_keycode = setup.min_keycode;
    let max_keycode = setup.max_keycode;

    let mapping = conn
        .get_keyboard_mapping(min_keycode, max_keycode - min_keycode + 1)?
        .reply()?;
    let keysyms_per_keycode = mapping.keysyms_per_keycode as usize;

    let mut k_ret = None;
    let mut k_space = None;
    let mut k_q = None;
    let mut k_j = None;
    let mut k_k = None;
    let mut k_p = None;
    let mut k_b = None;
    let mut k_minus = None;
    let mut k_backslash = None;
    let mut key_map = Vec::new(); // Stores (keycode, ws_idx)

    for (i, code) in (min_keycode..=max_keycode).enumerate() {
        let start = i * keysyms_per_keycode;
        for &sym in &mapping.keysyms[start..start + keysyms_per_keycode] {
            if sym == XK_RETURN && k_ret.is_none() {
                k_ret = Some(code);
            }
            if sym == XK_SPACE && k_space.is_none() {
                k_space = Some(code);
            }
            if sym == XK_Q && k_q.is_none() {
                k_q = Some(code);
            }
            if sym == XK_J && k_j.is_none() {
                k_j = Some(code);
            }
            if sym == XK_K && k_k.is_none() {
                k_k = Some(code);
            }
            if sym == XK_P && k_p.is_none() {
                k_p = Some(code);
            }
            if sym == XK_B && k_b.is_none() {
                k_b = Some(code);
            }
            if sym == XK_MINUS && k_minus.is_none() {
                k_minus = Some(code);
            }
            if sym == XK_BACKSLASH && k_backslash.is_none() {
                k_backslash = Some(code);
            }
            if sym >= XK_1 && sym <= XK_9 {
                let ws_index = (sym - XK_1) as usize;
                key_map.push((code, ws_index));
            }
        }
    }

    Ok((
        k_ret,
        k_space,
        k_q,
        k_j,
        k_k,
        k_p,
        k_b,
        k_minus,
        k_backslash,
        key_map,
    ))
}

fn setup_cursor(
    conn: &impl Connection,
    screen: &xproto::Screen,
) -> Result<(), Box<dyn std::error::Error>> {
    let font_id = conn.generate_id()?;
    conn.open_font(font_id, b"cursor")?;

    let cursor_id = conn.generate_id()?;

    conn.create_glyph_cursor(
        cursor_id, font_id, font_id, 68, 69, 0, 0, 0, 65535, 65535, 65535,
    )?;

    let changes = xproto::ChangeWindowAttributesAux::new().cursor(cursor_id);
    conn.change_window_attributes(screen.root, &changes)?;
    conn.close_font(font_id)?;
    Ok(())
}
