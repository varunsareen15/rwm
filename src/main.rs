mod bar;
mod config;
mod layout;
mod state;
mod workspace;

use config::Config;
use simplelog::{
    ColorChoice, CombinedLogger, Config as LogConfig, LevelFilter, TermLogger, TerminalMode,
    WriteLogger,
};
use state::WindowManager;
use std::collections::HashMap;
use std::fs::File;
use std::process::Command;
use std::thread;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{
    self, ClientMessageData, ClientMessageEvent, ConnectionExt, ModMask,
};

#[derive(Debug, Clone)]
enum Action {
    Spawn(String),
    KillFocused,
    Quit,
    FocusNext,
    FocusPrev,
    MoveWindowNext,
    MoveWindowPrev,
    CycleLayout,
    ToggleBar,
    SplitVertical,
    SplitHorizontal,
    PromoteMaster,
    Workspace(usize),
    MoveToWorkspace(usize),
}

fn parse_action(cmd: &str) -> Option<Action> {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    match parts[0] {
        "Spawn" => Some(Action::Spawn(parts[1..].join(" "))),
        "KillFocused" => Some(Action::KillFocused),
        "Quit" => Some(Action::Quit),
        "FocusNext" => Some(Action::FocusNext),
        "FocusPrev" => Some(Action::FocusPrev),
        "MoveWindowNext" => Some(Action::MoveWindowNext),
        "MoveWindowPrev" => Some(Action::MoveWindowPrev),
        "CycleLayout" => Some(Action::CycleLayout),
        "ToggleBar" => Some(Action::ToggleBar),
        "SplitHorizontal" => Some(Action::SplitHorizontal),
        "SplitVertical" => Some(Action::SplitVertical),
        "PromoteMaster" => Some(Action::PromoteMaster),
        "Workspace" => parts
            .get(1)
            .and_then(|s| s.parse().ok())
            .map(Action::Workspace),
        "MoveToWorkspace" => parts
            .get(1)
            .and_then(|s| s.parse().ok())
            .map(Action::MoveToWorkspace),
        _ => {
            log::warn!("Unknown action: {}", cmd);
            None
        }
    }
}

fn keysym_from_name(name: &str) -> u32 {
    match name {
        "Return" => 0xff0d,
        "Space" => 0x0020,
        "BackSpace" => 0xff08,
        "Tab" => 0xff09,
        "Escape" => 0xff1b,
        "Shift_L" => 0xffe1,
        "Shift_R" => 0xffe2,
        "Control_L" => 0xffe3,
        "Control_R" => 0xffe4,
        "minus" => 0x002d,
        "backslash" => 0x005c,
        "bar" => 0x007c,
        // Simple ascii mapping
        c if c.len() == 1 => {
            let ch = c.chars().next().unwrap();
            if ch.is_ascii_graphic() {
                u32::from(ch)
            } else {
                0
            }
        }
        _ => 0, // Unknown
    }
}

fn parse_keybind(bind: &str, mod_key_mask: ModMask) -> (u32, u16) {
    let mut mask = 0u16;
    let mut keysym = 0u32;

    for part in bind.split('+') {
        match part {
            "Mod" => mask |= u16::from(mod_key_mask),
            "Shift" => mask |= u16::from(ModMask::SHIFT),
            "Control" => mask |= u16::from(ModMask::CONTROL),
            "Alt" => mask |= u16::from(ModMask::M1),
            key => keysym = keysym_from_name(key),
        }
    }
    (keysym, mask)
}

fn detect_mod_key() -> ModMask {
    // Simplified detection for now
    if std::env::var("RWM_MOD").unwrap_or_default().to_lowercase() == "alt" {
        ModMask::M1
    } else {
        ModMask::M4 // Super
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Info,
            LogConfig::default(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        WriteLogger::new(
            LevelFilter::Info,
            LogConfig::default(),
            File::create("/tmp/rwm.log")?,
        ),
    ])?;

    let config = Config::load();

    let (conn, screen_num) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen_num];
    let root_win = screen.root;
    let mod_mask = detect_mod_key();

    log::info!(
        "Connected. Screen: {}x{}",
        screen.width_in_pixels,
        screen.height_in_pixels
    );

    state::WindowManager::setup_cursor(&conn, screen)?;
    let change = xproto::ChangeWindowAttributesAux::new().event_mask(
        xproto::EventMask::SUBSTRUCTURE_REDIRECT | xproto::EventMask::SUBSTRUCTURE_NOTIFY,
    );
    conn.change_window_attributes(screen.root, &change)?;

    thread::spawn(move || {
        // Open a separate connection for the thread
        match x11rb::connect(None) {
            Ok((timer_conn, _)) => {
                loop {
                    thread::sleep(Duration::from_secs(1));

                    // Create a dummy event to wake up the main loop
                    let event = ClientMessageEvent {
                        response_type: x11rb::protocol::xproto::CLIENT_MESSAGE_EVENT,
                        format: 32,
                        sequence: 0,
                        window: root_win,
                        type_: x11rb::protocol::xproto::AtomEnum::STRING.into(), // Using generic STRING atom
                        data: ClientMessageData::from([0, 0, 0, 0, 0]),
                    };

                    // Send event and flush
                    let _ = timer_conn.send_event(
                        false,
                        root_win,
                        x11rb::protocol::xproto::EventMask::NO_EVENT,
                        &event,
                    );
                    let _ = timer_conn.flush();
                }
            }
            Err(e) => log::error!("Timer thread failed to connect to X11: {}", e),
        }
    });

    let mut key_actions: HashMap<(u16, u8), Action> = HashMap::new();

    let mut needed_keysyms = Vec::new();
    let mut raw_bindings = Vec::new();

    for (key_str, action_str) in &config.bindings {
        if let Some(action) = parse_action(action_str) {
            let (sym, mask) = parse_keybind(key_str, mod_mask);
            if sym != 0 {
                needed_keysyms.push(sym);
                raw_bindings.push((sym, mask, action));
            }
        }
    }

    let min_keycode = conn.setup().min_keycode;
    let max_keycode = conn.setup().max_keycode;
    let mapping = conn
        .get_keyboard_mapping(min_keycode, max_keycode - min_keycode + 1)?
        .reply()?;
    let keysyms_per_keycode = mapping.keysyms_per_keycode as usize;

    let mut sym_to_code: HashMap<u32, u8> = HashMap::new();
    for (i, code) in (min_keycode..=max_keycode).enumerate() {
        let start = i * keysyms_per_keycode;
        for &sym in &mapping.keysyms[start..start + keysyms_per_keycode] {
            if needed_keysyms.contains(&sym) && sym != 0 {
                sym_to_code.insert(sym, code);
            }
        }
    }

    let ignored_modifiers = [
        0,
        u16::from(ModMask::M2),
        u16::from(ModMask::LOCK),
        u16::from(ModMask::M2 | ModMask::LOCK),
    ];

    for (sym, mask, action) in raw_bindings {
        if let Some(&code) = sym_to_code.get(&sym) {
            key_actions.insert((mask, code), action);

            for ignored in ignored_modifiers {
                conn.grab_key(
                    true,
                    screen.root,
                    ModMask::from(mask | ignored),
                    code,
                    xproto::GrabMode::ASYNC,
                    xproto::GrabMode::ASYNC,
                )
                .ok();
            }
        } else {
            log::warn!("Could not find keycode for keysym: {}", sym);
        }
    }
    conn.flush()?;
    log::info!("RWM STARTED with {} keybinds", key_actions.len());

    let mut wm_state = WindowManager::new(&conn, screen, config.clone())?;

    loop {
        conn.flush()?;
        let event = conn.wait_for_event()?;

        match event {
            Event::KeyPress(evt) => {
                let mask = evt.state;
                // Clean mask of Lock/NumLock for lookup
                let clean_mask =
                    u16::from(mask) & !(u16::from(ModMask::M2) | u16::from(ModMask::LOCK));

                if let Some(action) = key_actions.get(&(clean_mask, evt.detail)) {
                    log::info!("Executing: {:?}", action);
                    match action {
                        Action::Spawn(cmd) => spawn(cmd),
                        Action::KillFocused => wm_state.kill_focused_window(&conn)?,
                        Action::Quit => {
                            wm_state.kill_all_windows(&conn)?;
                            break;
                        }
                        Action::FocusNext => {
                            wm_state.cycle_focus(&conn, state::FocusDirection::Next)?
                        }
                        Action::FocusPrev => {
                            wm_state.cycle_focus(&conn, state::FocusDirection::Prev)?
                        }
                        Action::MoveWindowNext => {
                            wm_state.move_focused_window(&conn, state::FocusDirection::Next)?
                        }
                        Action::MoveWindowPrev => {
                            wm_state.move_focused_window(&conn, state::FocusDirection::Prev)?
                        }
                        Action::CycleLayout => wm_state.cycle_layout(&conn)?,
                        Action::ToggleBar => wm_state.toggle_bar(&conn)?,
                        Action::SplitHorizontal => {
                            wm_state.set_split_direction(&conn, workspace::SplitAxis::Horizontal)?
                        }
                        Action::SplitVertical => {
                            wm_state.set_split_direction(&conn, workspace::SplitAxis::Vertical)?
                        }
                        Action::PromoteMaster => wm_state.promote_focused_to_master(&conn)?,
                        Action::Workspace(i) => wm_state.switch_workspace(&conn, i - 1)?, // Config is 1-based, internal is 0-based
                        Action::MoveToWorkspace(i) => {
                            wm_state.move_window_to_workspace(&conn, i - 1)?
                        }
                    }
                }
            }
            Event::MapRequest(evt) => wm_state.handle_map_request(&conn, evt.window)?,
            Event::DestroyNotify(evt) => wm_state.handle_destroy_notify(&conn, evt.window)?,
            Event::Expose(evt) => wm_state.handle_expose(&conn, evt)?,
            Event::EnterNotify(evt) => wm_state.handle_enter_notify(&conn, evt)?,
            Event::ButtonPress(evt) => {
                if evt.event == wm_state.bar.window {
                    wm_state.handle_bar_click(&conn, evt.event_x)?;
                }
            }
            Event::ClientMessage(_) => {
                wm_state.handle_timer_tick(&conn)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn spawn(command: &str) {
    match Command::new("sh").arg("-c").arg(command).spawn() {
        Ok(_) => log::info!("Spawned {}", command),
        Err(e) => log::error!("Failed to spawn {}: {}", command, e),
    }
}
