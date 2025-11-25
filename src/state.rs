use x11rb::connection::Connection;
use x11rb::protocol::xproto::{Screen, Window, ConnectionExt, InputFocus, Time};
use crate::layout;

pub struct WindowManager {
    managed_windows: Vec<Window>,
    focused_window: Option<Window>,
    screen_width: u16,
    screen_height: u16,
}

impl WindowManager {
    pub fn new(screen: &Screen) -> Self {
        Self {
            managed_windows: Vec::new(),
            focused_window: None,
            screen_width: screen.width_in_pixels,
            screen_height: screen.height_in_pixels,
        }
    }

    pub fn handle_map_request<C: Connection>(
        &mut self,
        conn: &C,
        window: Window,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !self.managed_windows.contains(&window) {
            self.managed_windows.push(window);
        }

        conn.map_window(window)?;
        
        // Focus the new window
        self.set_focus(conn, window)?;

        self.refresh_layout(conn)?;
        Ok(())
    }

    pub fn handle_destroy_notify<C: Connection>(
        &mut self,
        conn: &C,
        window: Window,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(pos) = self.managed_windows.iter().position(|&w| w == window) {
            self.managed_windows.remove(pos);
            
            if self.focused_window == Some(window) {
                self.focused_window = None;
            }

            self.refresh_layout(conn)?;
        }
        Ok(())
    }

    pub fn kill_focused_window<C: Connection>(&self, conn: &C) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(window) = self.focused_window {
            conn.kill_client(window)?;
        }
        Ok(())
    }

    fn set_focus<C: Connection>(&mut self, conn: &C, window: Window) -> Result<(), Box<dyn std::error::Error>> {
        self.focused_window = Some(window);
        // FIX: Use 0u32 instead of `0 as Time`. 
        // In X11, time 0 means "CurrentTime".
        conn.set_input_focus(InputFocus::POINTER_ROOT, window, 0u32)?;
        Ok(())
    }

    fn refresh_layout<C: Connection>(
        &self,
        conn: &C,
    ) -> Result<(), Box<dyn std::error::Error>> {
        layout::tile_windows(
            conn, 
            &self.managed_windows, 
            self.screen_width, 
            self.screen_height
        )
    }
}
