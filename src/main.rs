mod layout;
mod state;

use std::process::Command;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{self, ConnectionExt, ModMask};
use state::WindowManager;

// Constants for X11 Keysyms
const XK_RETURN: u32 = 0xff0d; 
const XK_Q: u32      = 0x0071; 

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let (conn, screen_num) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen_num];

    log::info!("Connected. Screen: {}x{}", screen.width_in_pixels, screen.height_in_pixels);

    let change = xproto::ChangeWindowAttributesAux::new()
        .event_mask(xproto::EventMask::SUBSTRUCTURE_REDIRECT | xproto::EventMask::SUBSTRUCTURE_NOTIFY);
    
    conn.change_window_attributes(screen.root, &change)?;

    // --- Setup Keybinds ---
    let (key_return, key_q) = scan_key_codes(&conn)?;
    
    // 1. Super + Enter (Spawn Kitty)
    if let Some(code) = key_return {
        grab_key(&conn, screen.root, code, ModMask::M4)?; 
    }
    
    if let Some(code) = key_q {
        // 2. Super + Shift + Q (Close Focused Window)
        grab_key(&conn, screen.root, code, ModMask::M4 | ModMask::SHIFT)?;

        // 3. Super + Ctrl + Q (Quit Window Manager)
        grab_key(&conn, screen.root, code, ModMask::M4 | ModMask::CONTROL)?;
    }
    
    conn.flush()?;
    // ----------------------

    let mut wm_state = WindowManager::new(screen);
    log::info!("WM Started.");
    log::info!("  [Super + Enter]    -> Spawn Kitty");
    log::info!("  [Super + Shift + Q]-> Close Focused Window");
    log::info!("  [Super + Ctrl + Q] -> Quit WM");

    loop {
        conn.flush()?;
        let event = conn.wait_for_event()?;

        match event {
            Event::MapRequest(evt) => {
                wm_state.handle_map_request(&conn, evt.window)?;
            },
            Event::DestroyNotify(evt) => {
                wm_state.handle_destroy_notify(&conn, evt.window)?;
            },
            Event::KeyPress(evt) => {
                // FIX: Convert types to u16 to compare them safely
                let modifiers = u16::from(evt.state);
                let key = evt.detail;      

                // Define masks as u16 for easy comparison
                let mod_super = u16::from(ModMask::M4);
                let mod_shift = u16::from(ModMask::SHIFT);
                let mod_ctrl  = u16::from(ModMask::CONTROL);

                if Some(key) == key_return && (modifiers & mod_super != 0) {
                    spawn_terminal();
                } 
                else if Some(key) == key_q {
                    if (modifiers & mod_super != 0) && (modifiers & mod_shift != 0) {
                        log::info!("Killing focused window");
                        wm_state.kill_focused_window(&conn)?;
                    } 
                    else if (modifiers & mod_super != 0) && (modifiers & mod_ctrl != 0) {
                        log::info!("Quit sequence detected. Exiting...");
                        break; 
                    }
                }
            },
            _ => {}
        }
    }
    Ok(())
}

fn grab_key(conn: &impl Connection, root: xproto::Window, keycode: u8, modifiers: ModMask) -> Result<(), Box<dyn std::error::Error>> {
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

fn scan_key_codes(conn: &impl Connection) -> Result<(Option<u8>, Option<u8>), Box<dyn std::error::Error>> {
    let setup = conn.setup();
    let min_keycode = setup.min_keycode;
    let max_keycode = setup.max_keycode;

    let mapping = conn.get_keyboard_mapping(min_keycode, max_keycode - min_keycode + 1)?.reply()?;
    let keysyms_per_keycode = mapping.keysyms_per_keycode as usize;

    let mut return_code = None;
    let mut q_code = None;

    for (i, code) in (min_keycode..=max_keycode).enumerate() {
        let start = i * keysyms_per_keycode;
        for &sym in &mapping.keysyms[start..start + keysyms_per_keycode] {
            if sym == XK_RETURN && return_code.is_none() {
                return_code = Some(code);
            }
            if sym == XK_Q && q_code.is_none() {
                q_code = Some(code);
            }
        }
    }

    Ok((return_code, q_code))
}
