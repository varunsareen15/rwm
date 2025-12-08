use crate::bar::Bar;
use crate::layout::{self, Layout};
use crate::workspace::Workspace;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    ChangeWindowAttributesAux, ConfigureWindowAux, ConnectionExt, EnterNotifyEvent, EventMask,
    ExposeEvent, InputFocus, NotifyDetail, NotifyMode, Screen, StackMode, Window,
};

pub enum FocusDirection {
    Next,
    Prev,
}

pub struct WindowManager {
    workspaces: Vec<Workspace>,
    active_workspace_idx: usize,
    focused_window: Option<Window>,
    pub bar: Bar,
    screen_width: u16,
    screen_height: u16,
    root: Window,
    current_top_gap: u16,
}

impl WindowManager {
    pub fn new<C: Connection>(
        conn: &C,
        screen: &Screen,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut workspaces = Vec::new();
        for _ in 0..9 {
            workspaces.push(Workspace::new());
        }

        let bar = Bar::new(conn, screen)?;
        bar.draw(conn, 0, 9)?;

        Ok(Self {
            workspaces,
            active_workspace_idx: 0,
            focused_window: None,
            bar,
            screen_width: screen.width_in_pixels,
            screen_height: screen.height_in_pixels,
            root: screen.root,
            current_top_gap: 20,
        })
    }

    pub fn handle_map_request<C: Connection>(
        &mut self,
        conn: &C,
        window: Window,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let active_ws = &mut self.workspaces[self.active_workspace_idx];

        if !active_ws.windows.contains(&window) {
            active_ws.windows.push(window);
        }

        let changes = ChangeWindowAttributesAux::new()
            .event_mask(EventMask::ENTER_WINDOW | EventMask::STRUCTURE_NOTIFY);
        conn.change_window_attributes(window, &changes)?;

        conn.map_window(window)?;
        // Focus the new window
        self.set_focus(conn, window)?;
        self.bar
            .draw(conn, self.active_workspace_idx, self.workspaces.len())?;
        self.refresh_layout(conn)?;
        Ok(())
    }

    pub fn handle_expose<C: Connection>(
        &mut self,
        conn: &C,
        event: ExposeEvent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if event.window == self.bar.window {
            self.bar
                .draw(conn, self.active_workspace_idx, self.workspaces.len())?;
        }
        Ok(())
    }

    pub fn handle_enter_notify<C: Connection>(
        &mut self,
        conn: &C,
        event: EnterNotifyEvent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if event.mode != NotifyMode::NORMAL || event.detail == NotifyDetail::INFERIOR {
            return Ok(());
        }

        let active_ws = &self.workspaces[self.active_workspace_idx];
        if active_ws.windows.contains(&event.event) {
            self.set_focus(conn, event.event)?;
        }
        Ok(())
    }

    pub fn handle_destroy_notify<C: Connection>(
        &mut self,
        conn: &C,
        window: Window,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let active_ws = &mut self.workspaces[self.active_workspace_idx];
        // Find if the destroyed window was in our list
        if let Some(pos) = active_ws.windows.iter().position(|&w| w == window) {
            active_ws.windows.remove(pos);
            // If the destroyed window was the one with focus...
            if self.focused_window == Some(window) {
                // ...try to focus the previous window in the list (or the last one)
                // If the list is empty, this returns None, which is correct.
                let next_window = active_ws.windows.last().copied();
                if let Some(win) = next_window {
                    self.set_focus(conn, win)?;
                } else {
                    self.focused_window = None;
                    conn.set_input_focus(InputFocus::POINTER_ROOT, self.root, 0u32)?;
                }
            }
            self.refresh_layout(conn)?;
        }
        Ok(())
    }

    pub fn switch_workspace<C: Connection>(
        &mut self,
        conn: &C,
        index: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if index == self.active_workspace_idx || index >= self.workspaces.len() {
            return Ok(());
        }

        // Hide previous workspace
        for window in &self.workspaces[self.active_workspace_idx].windows {
            conn.unmap_window(*window)?;
        }

        self.active_workspace_idx = index;

        // Show new workspace
        for window in &self.workspaces[self.active_workspace_idx].windows {
            conn.map_window(*window)?;
        }

        self.bar
            .draw(conn, self.active_workspace_idx, self.workspaces.len())?;

        // Focus workspace
        if let Some(&window) = self.workspaces[self.active_workspace_idx].windows.last() {
            self.set_focus(conn, window)?;
        } else {
            self.focused_window = None;
            conn.set_input_focus(InputFocus::POINTER_ROOT, self.root, 0u32)?;
        }

        self.refresh_layout(conn)?;
        Ok(())
    }

    pub fn move_window_to_workspace<C: Connection>(
        &mut self,
        conn: &C,
        target_index: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if target_index == self.active_workspace_idx || target_index >= self.workspaces.len() {
            return Ok(());
        }
        if let Some(window) = self.focused_window {
            let active_ws = &mut self.workspaces[self.active_workspace_idx];
            if let Some(pos) = active_ws.windows.iter().position(|&w| w == window) {
                active_ws.windows.remove(pos);
            }
            conn.unmap_window(window)?;
            self.workspaces[target_index].windows.push(window);
            let active_ws = &self.workspaces[self.active_workspace_idx];
            if let Some(&last) = active_ws.windows.last() {
                self.set_focus(conn, last)?;
            } else {
                self.focused_window = None;
                conn.set_input_focus(InputFocus::POINTER_ROOT, self.root, 0u32)?;
            }
            self.refresh_layout(conn)?;
        }
        Ok(())
    }

    pub fn cycle_layout<C: Connection>(
        &mut self,
        conn: &C,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let active_ws = &mut self.workspaces[self.active_workspace_idx];
        active_ws.layout = match active_ws.layout {
            Layout::MasterStack => Layout::VerticalStack,
            Layout::VerticalStack => Layout::Monocle,
            Layout::Monocle => Layout::MasterStack,
        };
        // Changing layout might require restacking so refocus to ensure focused window stays on
        // top if needed
        if let Some(win) = self.focused_window {
            self.set_focus(conn, win)?;
        }
        self.refresh_layout(conn)?;
        Ok(())
    }

    pub fn cycle_focus<C: Connection>(
        &mut self,
        conn: &C,
        dir: FocusDirection,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let active_ws = &mut self.workspaces[self.active_workspace_idx];
        if active_ws.windows.is_empty() {
            return Ok(());
        }

        // Find the index of the currently focused window
        let current_index = match self.focused_window {
            Some(w) => active_ws.windows.iter().position(|&win| win == w),
            None => None,
        };

        // Calculate the next index
        let next_index = match current_index {
            Some(i) => match dir {
                FocusDirection::Next => (i + 1) % active_ws.windows.len(),
                // Logic for wrappign backwards (e.g. 0 -> last)
                FocusDirection::Prev => (i + active_ws.windows.len() - 1) % active_ws.windows.len(),
            },
            None => 0, // If nothing is focused, start at 0
        };

        // Set the focus
        let next_window = active_ws.windows[next_index];
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
        let values = ConfigureWindowAux::new().stack_mode(StackMode::ABOVE);
        conn.configure_window(window, &values)?;
        Ok(())
    }

    fn refresh_layout<C: Connection>(&self, conn: &C) -> Result<(), Box<dyn std::error::Error>> {
        let active_ws = &self.workspaces[self.active_workspace_idx];
        layout::apply_layout(
            conn,
            active_ws.layout,
            &active_ws.windows,
            self.screen_width,
            self.screen_height,
            self.current_top_gap,
        )
    }

    pub fn promote_focused_to_master<C: Connection>(
        &mut self,
        conn: &C,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let active_ws = &mut self.workspaces[self.active_workspace_idx];
        // Need at least 2 active windows to swap anything
        if active_ws.windows.len() < 2 {
            return Ok(());
        }

        if let Some(focused) = self.focused_window {
            if let Some(pos) = active_ws.windows.iter().position(|&w| w == focused) {
                // If we are not Master (index 0), swap with Master
                if pos > 0 {
                    active_ws.windows.swap(0, pos);
                } else {
                    // If we are the Master, swap with the top of the stack (index 1).
                    active_ws.windows.swap(0, 1);
                }
                self.refresh_layout(conn)?;
            }
        }
        Ok(())
    }

    pub fn move_focused_window<C: Connection>(
        &mut self,
        conn: &C,
        dir: FocusDirection,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let active_ws = &mut self.workspaces[self.active_workspace_idx];
        let len = active_ws.windows.len();

        if len < 2 {
            return Ok(());
        }

        if let Some(focused) = self.focused_window {
            if let Some(pos) = active_ws.windows.iter().position(|&w| w == focused) {
                // Calculate the new index based on direction
                let new_pos = match dir {
                    FocusDirection::Next => (pos + 1) % len, // Move Down (Wrap to top)
                    FocusDirection::Prev => (pos + len - 1) % len, // Move Up (Wrap to bottom)
                };
                // Swap the windows in the vector
                active_ws.windows.swap(pos, new_pos);

                // Refresh layout to reflect the new order
                self.refresh_layout(conn)?;
            }
        }
        Ok(())
    }

    pub fn kill_all_windows<C: Connection>(
        &self,
        conn: &C,
    ) -> Result<(), Box<dyn std::error::Error>> {
        log::info!("Killing all managed windows before exit...");

        for ws in &self.workspaces {
            for &window in &ws.windows {
                let _ = conn.kill_client(window);
            }
        }

        conn.get_input_focus()?.reply()?;
        Ok(())
    }

    pub fn toggle_bar<C: Connection>(
        &mut self,
        conn: &C,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.current_top_gap > 0 {
            self.current_top_gap = 0;
            conn.unmap_window(self.bar.window)?;
        } else {
            self.current_top_gap = 20;
            conn.map_window(self.bar.window)?;
            self.bar.draw(conn, self.active_workspace_idx, self.workspaces.len())?;
        }
        self.refresh_layout(conn)?;
        Ok(())
    }

    pub fn handle_bar_click<C: Connection>(&mut self, conn: &C, x: i16) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ws_idx) = self.bar.get_clicked_workspace(x) {
            self.switch_workspace(conn, ws_idx)?;
        }
        Ok(())
    }
}
