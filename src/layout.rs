use crate::workspace::SplitAxis;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConfigureWindowAux, ConnectionExt, Window};

const BORDER_WIDTH: u16 = 0;

#[derive(Debug, Clone, Copy)]
pub enum Layout {
    VerticalStack, // Every window same height
    MasterStack,   // One Master on left, stack on right
    Monocle,       // Every window takes whole screen, stacked on top of each other
    Dwindle,       // Fibonacci layout but manual selection of where next window opens
}

// Main entry point that dispatches to specific layout functions
pub fn apply_layout<C: Connection>(
    conn: &C,
    layout_kind: Layout,
    windows: &[Window],
    screen_width: u16,
    screen_height: u16,
    top_gap: u16,
    split_history: &[SplitAxis],
) -> Result<(), Box<dyn std::error::Error>> {
    let usable_height = screen_height - top_gap;

    match layout_kind {
        Layout::Dwindle => tile_dwindle(
            conn,
            windows,
            screen_width,
            usable_height,
            top_gap,
            split_history,
        ),
        Layout::VerticalStack => {
            tile_vertical_stack(conn, windows, screen_width, usable_height, top_gap)
        }
        Layout::MasterStack => {
            tile_master_stack(conn, windows, screen_width, usable_height, top_gap)
        }
        Layout::Monocle => tile_monocle(conn, windows, screen_width, usable_height, top_gap),
    }
}

pub fn tile_vertical_stack<C: Connection>(
    conn: &C,
    windows: &[Window],
    screen_width: u16,
    usable_height: u16,
    top_gap: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let num_windows = windows.len() as u16;

    if num_windows == 0 {
        return Ok(());
    }

    let height_per_window = usable_height / num_windows;
    let mut y_offset = top_gap;

    for (i, &window) in windows.iter().enumerate() {
        let slot_height = if i == (num_windows - 1) as usize {
            (usable_height + top_gap) - y_offset
        } else {
            height_per_window
        };

        let final_width = (screen_width as u32).saturating_sub((2 * BORDER_WIDTH) as u32);
        let final_height = (slot_height as u32).saturating_sub((2 * BORDER_WIDTH) as u32);

        let changes = ConfigureWindowAux::new()
            .x(0)
            .y(y_offset as i32)
            .width(final_width)
            .height(final_height)
            .border_width(BORDER_WIDTH as u32);

        conn.configure_window(window, &changes)?;
        y_offset += slot_height;
    }
    Ok(())
}

pub fn tile_master_stack<C: Connection>(
    conn: &C,
    windows: &[Window],
    screen_width: u16,
    usable_height: u16,
    top_gap: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let num_windows = windows.len();
    if num_windows == 0 {
        return Ok(());
    }

    // If only one window, it takes the full screen
    if num_windows == 1 {
        return tile_vertical_stack(conn, windows, screen_width, usable_height, top_gap);
    }

    // Parameters
    let master_ratio = 0.55; // Master takes 55% width
    let master_width = (screen_width as f32 * master_ratio) as u16;
    let stack_width = screen_width - master_width;

    let master_final_w = (master_width as u32).saturating_sub((2 * BORDER_WIDTH) as u32);
    let master_final_h = (usable_height as u32).saturating_sub((2 * BORDER_WIDTH) as u32);

    // Configure the Master Window (Index 0)
    let master_changes = ConfigureWindowAux::new()
        .x(0)
        .y(top_gap as i32)
        .width(master_final_w)
        .height(master_final_h)
        .border_width(BORDER_WIDTH as u32);

    conn.configure_window(windows[0], &master_changes)?;

    // Configure the Stack Windows (Indices 1..n)
    let stack_windows = &windows[1..];
    let num_stack = stack_windows.len() as u16;
    let height_per_stack = usable_height / num_stack;
    let mut y_offset = top_gap;
    let stack_final_w = (stack_width as u32).saturating_sub((2 * BORDER_WIDTH) as u32);

    for (i, &window) in stack_windows.iter().enumerate() {
        let slot_height = if i == (num_stack - 1) as usize {
            (usable_height + top_gap) - y_offset
        } else {
            height_per_stack
        };

        let stack_final_h = (slot_height as u32).saturating_sub((2 * BORDER_WIDTH) as u32);

        let changes = ConfigureWindowAux::new()
            .x(master_width as i32)
            .y(y_offset as i32)
            .width(stack_final_w)
            .height(stack_final_h)
            .border_width(BORDER_WIDTH as u32);

        conn.configure_window(window, &changes)?;
        y_offset += slot_height;
    }
    Ok(())
}

fn tile_monocle<C: Connection>(
    conn: &C,
    windows: &[Window],
    screen_width: u16,
    usable_height: u16,
    top_gap: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    // Every Window gets full screen dimensions
    let changes = ConfigureWindowAux::new()
        .x(0)
        .y(top_gap as i32)
        .width(screen_width as u32)
        .height(usable_height as u32)
        .border_width(0);

    for &window in windows {
        conn.configure_window(window, &changes)?;
    }
    Ok(())
}

pub fn tile_dwindle<C: Connection>(
    conn: &C,
    windows: &[Window],
    screen_width: u16,
    usable_height: u16,
    top_gap: u16,
    split_history: &[SplitAxis],
) -> Result<(), Box<dyn std::error::Error>> {
    let num_windows = windows.len();
    if num_windows == 0 {
        return Ok(());
    }

    let mut x = 0;
    let mut y = top_gap as i32;
    let mut width = screen_width as u32;
    let mut height = usable_height as u32;

    for (i, &window) in windows.iter().enumerate() {
        if i == num_windows - 1 {
            let final_w = width.saturating_sub((2 * BORDER_WIDTH) as u32);
            let final_h = height.saturating_sub((2 * BORDER_WIDTH) as u32);
            let changes = ConfigureWindowAux::new()
                .x(x)
                .y(y)
                .width(final_w)
                .height(final_h)
                .border_width(BORDER_WIDTH as u32);
            conn.configure_window(window, &changes)?;
        } else {
            let axis = if i < split_history.len() {
                split_history[i]
            } else {
                SplitAxis::Vertical
            };

            let (w, h) = match axis {
                SplitAxis::Horizontal => {
                    let split_w = width / 2;
                    width -= split_w;
                    (split_w, height)
                }
                SplitAxis::Vertical => {
                    let split_h = height / 2;
                    height -= split_h;
                    (width, split_h)
                }
            };

            let final_w = w.saturating_sub((2 * BORDER_WIDTH) as u32);
            let final_h = h.saturating_sub((2 * BORDER_WIDTH) as u32);

            let changes = ConfigureWindowAux::new()
                .x(x)
                .y(y)
                .width(final_w)
                .height(final_h)
                .border_width(BORDER_WIDTH as u32);
            conn.configure_window(window, &changes)?;

            match axis {
                SplitAxis::Horizontal => x += w as i32,
                SplitAxis::Vertical => y += h as i32,
            }
        }
    }
    Ok(())
}
