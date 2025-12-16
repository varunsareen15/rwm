use crate::bar::Bar;
use crate::layout::{self, Layout};
use crate::workspace::{SplitAxis, Workspace};
use std::process::Command;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    self, AtomEnum, ChangeWindowAttributesAux, ConfigureWindowAux, ConnectionExt, EnterNotifyEvent,
    EventMask, ExposeEvent, InputFocus, NotifyDetail, NotifyMode, Screen, StackMode, Window,
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
    pending_split: SplitAxis,
    last_mouse_pos: Option<(i16, i16)>,
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

        let wm = Self {
            workspaces,
            active_workspace_idx: 0,
            focused_window: None,
            bar,
            screen_width: screen.width_in_pixels,
            screen_height: screen.height_in_pixels,
            root: screen.root,
            current_top_gap: 20,
            pending_split: SplitAxis::Vertical,
            last_mouse_pos: None,
        };

        // Initial Draw
        wm.update_bar(conn)?;

        Ok(wm)
    }

    pub fn update_bar<C: Connection>(&self, conn: &C) -> Result<(), Box<dyn std::error::Error>> {
        // 1. Get Layout String
        let active_ws = &self.workspaces[self.active_workspace_idx];
        let layout_str = match active_ws.layout {
            Layout::MasterStack => "[Master]".to_string(),
            Layout::VerticalStack => "[Vertical]".to_string(),
            Layout::Monocle => "[Monocle]".to_string(),
            Layout::Dwindle => match self.pending_split {
                SplitAxis::Vertical => "[Dwindle -]".to_string(),
                SplitAxis::Horizontal => "[Dwindle |]".to_string(),
            },
        };

        // 2. Get Window Title
        let mut title = String::new();
        if let Some(window) = self.focused_window {
            if let Ok(reply) =
                conn.get_property(false, window, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 1024)
            {
                if let Ok(prop) = reply.reply() {
                    title = String::from_utf8_lossy(&prop.value).to_string();
                }
            }
        }

        // 3. Get Time (using `date` command as a simple workaround without extra dependencies)
        let time_output = Command::new("date").arg("+%H:%M").output();
        let time_str = match time_output {
            Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            Err(_) => "00:00".to_string(),
        };

        self.bar.draw(
            conn,
            self.active_workspace_idx,
            self.workspaces.len(),
            &layout_str,
            &title,
            &time_str,
        )?;
        Ok(())
    }

    pub fn handle_map_request<C: Connection>(
        &mut self,
        conn: &C,
        window: Window,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let existing_ws_idx = self
            .workspaces
            .iter()
            .position(|ws| ws.windows.contains(&window));

        if let Some(idx) = existing_ws_idx {
            if idx != self.active_workspace_idx {
                self.switch_workspace(conn, idx)?;
            }

            conn.map_window(window)?;
            self.set_focus(conn, window)?;
            self.refresh_layout(conn)?;
            self.update_bar(conn)?;
            return Ok(());
        }

        let active_ws = &mut self.workspaces[self.active_workspace_idx];
        active_ws.windows.push(window);
        active_ws.split_history.push(self.pending_split);

        let changes = ChangeWindowAttributesAux::new().event_mask(
            EventMask::ENTER_WINDOW | EventMask::STRUCTURE_NOTIFY | EventMask::PROPERTY_CHANGE,
        );
        conn.change_window_attributes(window, &changes)?;

        conn.map_window(window)?;
        self.set_focus(conn, window)?;
        self.update_bar(conn)?;
        self.refresh_layout(conn)?;
        Ok(())
    }

    pub fn handle_expose<C: Connection>(
        &mut self,
        conn: &C,
        event: ExposeEvent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if event.window == self.bar.window {
            self.update_bar(conn)?;
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

        if let Some(last) = self.last_mouse_pos {
            if last == (event.root_x, event.root_y) {
                return Ok(());
            }
        }

        self.last_mouse_pos = Some((event.root_x, event.root_y));

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
        for (i, ws) in self.workspaces.iter_mut().enumerate() {
            if let Some(pos) = ws.windows.iter().position(|&w| w == window) {
                ws.windows.remove(pos);
                if pos < ws.split_history.len() {
                    ws.split_history.remove(pos);
                }

                if i == self.active_workspace_idx {
                    self.refresh_layout(conn)?;
                }

                break;
            }
        }

        if self.focused_window == Some(window) {
            let active_ws = &self.workspaces[self.active_workspace_idx];
            if let Some(&new_focus) = active_ws.windows.last() {
                self.set_focus(conn, new_focus)?;
            } else {
                self.focused_window = None;
                conn.set_input_focus(InputFocus::POINTER_ROOT, self.root, 0u32)?;
            }
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

        let old_idx = self.active_workspace_idx;
        self.active_workspace_idx = index;
        self.refresh_layout(conn)?;

        // Show new workspace
        for window in &self.workspaces[self.active_workspace_idx].windows {
            conn.map_window(*window)?;
        }

        // Hide previous workspace
        for window in &self.workspaces[old_idx].windows {
            conn.unmap_window(*window)?;
        }

        self.update_bar(conn)?;

        // Focus workspace
        if let Some(&window) = self.workspaces[self.active_workspace_idx].windows.last() {
            self.set_focus(conn, window)?;
        } else {
            self.focused_window = None;
            conn.set_input_focus(InputFocus::POINTER_ROOT, self.root, 0u32)?;
        }

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
            let mut split_preference = SplitAxis::Vertical;

            if let Some(pos) = active_ws.windows.iter().position(|&w| w == window) {
                active_ws.windows.remove(pos);
                if pos < active_ws.split_history.len() {
                    split_preference = active_ws.split_history.remove(pos);
                }
            }

            conn.unmap_window(window)?;
            self.workspaces[target_index].windows.push(window);
            self.workspaces[target_index]
                .split_history
                .push(split_preference);
            self.refresh_layout(conn)?;

            let active_ws = &self.workspaces[self.active_workspace_idx];
            if let Some(&last) = active_ws.windows.last() {
                self.set_focus(conn, last)?;
            } else {
                self.focused_window = None;
                conn.set_input_focus(InputFocus::POINTER_ROOT, self.root, 0u32)?;
            }

            self.refresh_layout(conn)?;
            self.update_bar(conn)?;
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
            Layout::VerticalStack => Layout::Dwindle,
            Layout::Dwindle => Layout::Monocle,
            Layout::Monocle => Layout::MasterStack,
        };
        // Changing layout might require restacking so refocus to ensure focused window stays on
        // top if needed
        if let Some(win) = self.focused_window {
            self.set_focus(conn, win)?;
        }
        self.update_bar(conn)?;
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
        self.update_bar(conn)?;
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
        self.update_bar(conn)?;
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
            &active_ws.split_history,
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
            self.update_bar(conn)?;
        }
        self.refresh_layout(conn)?;
        Ok(())
    }

    pub fn handle_bar_click<C: Connection>(
        &mut self,
        conn: &C,
        x: i16,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ws_idx) = self.bar.get_clicked_workspace(x) {
            self.switch_workspace(conn, ws_idx)?;
        }
        Ok(())
    }

    pub fn set_split_direction<C: Connection>(
        &mut self,
        conn: &C,
        axis: SplitAxis,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.pending_split = axis;

        if let Some(ws) = self.workspaces.get_mut(self.active_workspace_idx) {
            if let Some(last_split) = ws.split_history.last_mut() {
                *last_split = axis;
            }
        }

        log::info!("Next window will split: {:?}", axis);

        self.update_bar(conn)?;

        Ok(())
    }

    pub fn setup_cursor(
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
}
