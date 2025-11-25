use x11rb::connection::Connection;
// FIX 2A: Import ConnectionExt for configure_window
use x11rb::protocol::xproto::{ConfigureWindowAux, ConnectionExt, Window}; 

/// Basic vertical tiling: splits the screen height evenly among all windows.
pub fn tile_windows<C: Connection>(
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

    for (i, &window) in windows.iter().enumerate() {
        let i = i as u16;

        let x = 0;
        let y = i * height_per_window;
        let width = screen_width;
        let height = height_per_window;

        // FIX 2B: Cast u16 (width/height) to u32 because x11rb expects u32 for these fields
        let changes = ConfigureWindowAux::new()
            .x(x as i32)
            .y(y as i32)
            .width(width as u32) 
            .height(height as u32)
            .border_width(1);

        // FIX 2C: configure_window is available because ConnectionExt is imported
        conn.configure_window(window, &changes)?; 
    }

    Ok(())
}
