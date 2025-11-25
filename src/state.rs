use crate::layout;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, InputFocus, Screen, Time, Window};

pub enum FocusDirection {
    Next,
    Prev,
}

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
        // Find if the destroyed window was in our list
        if let Some(pos) = self.managed_windows.iter().position(|&w| w == window) {
            self.managed_windows.remove(pos);

            // If the destroyed window was the one with focus...
            if self.focused_window == Some(window) {
                // ...try to focus the previous window in the list (or the last one)
                // If the list is empty, this returns None, which is correct.
                let next_window = self.managed_windows.last().copied();

                if let Some(win) = next_window {
                    self.set_focus(conn, win)?;
                } else {
                    self.focused_window = None;
                }
            }

            self.refresh_layout(conn)?;
        }
        Ok(())
    }

    pub fn cycle_focus<C: Connection>(
        &mut self,
        conn: &C,
        dir: FocusDirection,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.managed_windows.is_empty() {
            return Ok(());
        }

        // Find the index of the currently focused window
        let current_index = match self.focused_window {
            Some(w) => self.managed_windows.iter().position(|&win| win == w),
            None => None,
        };

        // Calculate the next index
        let next_index = match current_index {
            Some(i) => match dir {
                FocusDirection::Next => (i + 1) % self.managed_windows.len(),
                // Logic for wrappign backwards (e.g. 0 -> last)
                FocusDirection::Prev => {
                    (i + self.managed_windows.len() - 1) % self.managed_windows.len()
                }
            },
            None => 0, // If nothing is focused, start at 0
        };

        // Set the focus
        let next_window = self.managed_windows[next_index];
        self.set_focus(conn, next_window)?;

        Ok(())
    }

    pub fn kill_focused_window<C: Connection>(
        &self,
        conn: &C,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // We only try to kill if we actually have a focused window
        if let Some(window) = self.focused_window {
            conn.kill_client(window)?;
        }
        Ok(())
    }

    fn set_focus<C: Connection>(
        &mut self,
        conn: &C,
        window: Window,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.focused_window = Some(window);
        conn.set_input_focus(InputFocus::POINTER_ROOT, window, 0u32)?;
        Ok(())
    }

    fn refresh_layout<C: Connection>(&self, conn: &C) -> Result<(), Box<dyn std::error::Error>> {
        layout::tile_windows(
            conn,
            &self.managed_windows,
            self.screen_width,
            self.screen_height,
        )
    }
}
