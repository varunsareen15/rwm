use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConfigureWindowAux, ConnectionExt, Window};

#[derive(Debug, Clone, Copy)]
pub enum Layout {
    VerticalStack, // Every window same height
    MasterStack,   // One Master on left, stack on right
    Monocle,      // Every window takes whole screen, stacked on top of each other
}

// Main entry point that dispatches to specific layout functions
pub fn apply_layout<C: Connection>(
    conn: &C,
    layout_kind: Layout,
    windows: &[Window],
    screen_width: u16,
    screen_height: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    match layout_kind {
        Layout::VerticalStack => tile_vertical_stack(conn, windows, screen_width, screen_height),
        Layout::MasterStack => tile_master_stack(conn, windows, screen_width, screen_height),
        Layout::Monocle => tile_monocle(conn, windows, screen_width, screen_height),
    }
}

pub fn tile_vertical_stack<C: Connection>(
    conn: &C,
    windows: &[Window],
    screen_width: u16,
    screen_height: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let num_windows = windows.len() as u16;

    if num_windows == 0 {
        return Ok(());
    }

    let height_per_window = screen_height / num_windows;
    let mut y_offset = 0;

    for (i, &window) in windows.iter().enumerate() {
        let height = if i == (num_windows - 1) as usize {
            screen_height - y_offset
        } else {
            height_per_window
        };

        let changes = ConfigureWindowAux::new()
            .x(0)
            .y(y_offset as i32)
            .width(screen_width as u32)
            .height(height as u32)
            .border_width(1);

        conn.configure_window(window, &changes)?;
        y_offset += height;
    }
    Ok(())
}

pub fn tile_master_stack<C: Connection>(
    conn: &C,
    windows: &[Window],
    screen_width: u16,
    screen_height: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let num_windows = windows.len();
    if num_windows == 0 {
        return Ok(());
    }

    // If only one window, it takes the full screen
    if num_windows == 1 {
        return tile_vertical_stack(conn, windows, screen_width, screen_height);
    }

    // Parameters
    let master_ratio = 0.55; // Master takes 55% width
    let master_width = (screen_width as f32 * master_ratio) as u16;
    let stack_width = screen_width - master_width;

    // Configure the Master Window (Index 0)
    let master_changes = ConfigureWindowAux::new()
        .x(0)
        .y(0)
        .width(master_width as u32)
        .height(screen_height as u32)
        .border_width(1);

    conn.configure_window(windows[0], &master_changes)?;

    // Configure the Stack Windows (Indices 1..n)
    let stack_windows = &windows[1..];
    let num_stack = stack_windows.len() as u16;
    let height_per_stack = screen_height / num_stack;
    let mut y_offset = 0;

    for (i, &window) in stack_windows.iter().enumerate() {
        let height = if i == (num_stack - 1) as usize {
            screen_height - y_offset
        } else {
            height_per_stack
        };

        let changes = ConfigureWindowAux::new()
            .x(master_width as i32)
            .y(y_offset as i32)
            .width(stack_width as u32)
            .height(height as u32)
            .border_width(1);

        conn.configure_window(window, &changes)?;
        y_offset += height;
    }
    Ok(())
}

fn tile_monocle<C: Connection>(
    conn: &C,
    windows: &[Window],
    screen_width: u16,
    screen_height: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let width = screen_width as u32;
    let height = screen_height as u32;

    // Every WWindow gets full screen dimensions
    let changes = ConfigureWindowAux::new()
        .x(0)
        .y(0)
        .width(width)
        .height(height)
        .border_width(0);

    for &window in windows {
        conn.configure_window(window, &changes)?;
    }
    Ok(())
}
