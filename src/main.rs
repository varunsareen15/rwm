mod bar;
mod layout;
mod state;
mod workspace;

use state::WindowManager;
use std::process::Command;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{self, ConnectionExt, ModMask};

// Keysyms
const XK_RETURN: u32 = 0xff0d;
const XK_SPACE: u32 = 0x0020;
const XK_Q: u32 = 0x0071;
const XK_J: u32 = 0x006a;
const XK_K: u32 = 0x006b;
const XK_1: u32 = 0x0031;
const XK_9: u32 = 0x0039;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let (conn, screen_num) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen_num];

    log::info!(
        "Connected. Screen: {}x{}",
        screen.width_in_pixels,
        screen.height_in_pixels
    );

    let change = xproto::ChangeWindowAttributesAux::new().event_mask(
        xproto::EventMask::SUBSTRUCTURE_REDIRECT | xproto::EventMask::SUBSTRUCTURE_NOTIFY,
    );

    conn.change_window_attributes(screen.root, &change)?;

    // --- Setup Keybinds ---
    // We now scan for J and K as well
    let (k_ret, k_space, k_q, k_j, k_k, key_map) = scan_key_codes(&conn)?;

    // Grab keys
    if let Some(code) = k_ret {
        grab_key(&conn, screen.root, code, ModMask::M4)?;
    }

    if let Some(code) = k_space {
        grab_key(&conn, screen.root, code, ModMask::M4)?;
    }

    if let Some(code) = k_q {
        grab_key(&conn, screen.root, code, ModMask::M4 | ModMask::SHIFT)?;
        grab_key(&conn, screen.root, code, ModMask::M4 | ModMask::CONTROL)?;
    }

    if let Some(code) = k_j {
        grab_key(&conn, screen.root, code, ModMask::M4)?;
    }
    if let Some(code) = k_k {
        grab_key(&conn, screen.root, code, ModMask::M4)?;
    }

    for &(code, _) in &key_map {
        // Super + # (Switch)
        grab_key(&conn, screen.root, code, ModMask::M4)?;
        // Super + Shift + # (Move Window to ws)
        grab_key(&conn, screen.root, code, ModMask::M4 | ModMask::SHIFT)?;
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
            },
            Event::KeyPress(evt) => {
                let modifiers = u16::from(evt.state);
                let key = evt.detail;

                let mod_super = u16::from(ModMask::M4);
                let mod_shift = u16::from(ModMask::SHIFT);
                let mod_ctrl = u16::from(ModMask::CONTROL);

                if Some(key) == k_ret && (modifiers & mod_super != 0) {
                    spawn_terminal();
                } else if Some(key) == k_q {
                    if (modifiers & mod_super != 0) && (modifiers & mod_shift != 0) {
                        wm_state.kill_focused_window(&conn)?;
                    } else if (modifiers & mod_super != 0) && (modifiers & mod_ctrl != 0) {
                        break;
                    }
                } else if Some(key) == k_j && (modifiers & mod_super != 0) {
                    wm_state.cycle_focus(&conn, state::FocusDirection::Next)?;
                } else if Some(key) == k_k && (modifiers & mod_super != 0) {
                    wm_state.cycle_focus(&conn, state::FocusDirection::Prev)?;
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
    conn.grab_key(
        true,
        root,
        modifiers,
        keycode,
        xproto::GrabMode::ASYNC,
        xproto::GrabMode::ASYNC,
    )?;
    Ok(())
}

fn spawn_terminal() {
    match Command::new("kitty").spawn() {
        Ok(_) => log::info!("Spawned kitty"),
        Err(e) => log::error!("Failed to spawn kitty: {}", e),
    }
}

// Updated to return 4 keys
fn scan_key_codes(
    conn: &impl Connection,
) -> Result<
    (
        Option<u8>,
        Option<u8>,
        Option<u8>,
        Option<u8>,
        Option<u8>,
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
            if sym >= XK_1 && sym <= XK_9 {
                let ws_index = (sym - XK_1) as usize;
                key_map.push((code, ws_index));
            }
        }
    }

    Ok((k_ret, k_space, k_q, k_j, k_k, key_map))
}
