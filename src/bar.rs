use crate::config::BarConfig;
use std::process::Command;
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, Char2b, ConnectionExt, CreateGCAux, CreateWindowAux, EventMask, Font, Gcontext,
    Rectangle, Screen, Window, WindowClass,
};

pub struct ModuleState {
    pub last_output: String,
    pub last_update: Instant,
}

pub struct Bar {
    pub window: Window,
    gc: Gcontext,
    font: Font,
    width: u16,
    height: u16,
    config: BarConfig,
    module_states: Vec<ModuleState>,
}

impl Bar {
    pub fn new<C: Connection>(
        conn: &C,
        screen: &Screen,
        config: BarConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let window = conn.generate_id()?;
        let gc = conn.generate_id()?;
        let font = conn.generate_id()?;
        let height = 20;
        let width = screen.width_in_pixels;

        // Open Font (Try config font, fallback to fixed)
        if conn.open_font(font, config.font.as_bytes()).is_err() {
            log::warn!(
                "Could not load font '{}', falling back to 'fixed'",
                config.font
            );
            conn.open_font(font, b"fixed")?;
        }

        let win_aux = CreateWindowAux::new()
            .background_pixel(screen.black_pixel)
            .override_redirect(1)
            .event_mask(EventMask::EXPOSURE | EventMask::BUTTON_PRESS);

        conn.create_window(
            screen.root_depth,
            window,
            screen.root,
            0,
            0,
            width,
            height,
            0,
            WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &win_aux,
        )?;

        let gc_aux = CreateGCAux::new()
            .foreground(screen.white_pixel)
            .background(screen.black_pixel)
            .font(font)
            .graphics_exposures(0);

        conn.create_gc(gc, window, &gc_aux)?;
        conn.map_window(window)?;

        // Initialize Module States
        let module_states = config
            .modules
            .iter()
            .map(|_| ModuleState {
                last_output: String::new(),
                last_update: Instant::now() - Duration::from_secs(100), // Force immediate update
            })
            .collect();

        Ok(Self {
            window,
            gc,
            font,
            width,
            height,
            config,
            module_states,
        })
    }

    pub fn draw<C: Connection>(
        &mut self,
        conn: &C,
        active_idx: usize,
        _total_workspaces: usize,
        layout_name: &str,
        focused_window: Option<Window>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 1. Clear Bar
        conn.clear_area(false, self.window, 0, 0, self.width, self.height)?;

        let mut x_offset = 10i16;

        // 2. Draw Workspaces
        for (i, icon) in self.config.workspace_icons.iter().enumerate() {
            let is_active = i == active_idx;
            let display_text = if self.config.workspace_style == "Squares" {
                if is_active { "[x]" } else { "[ ]" }
            } else {
                icon.as_str()
            };

            // Highlight active workspace
            if is_active {
                let width = self.text_width(conn, display_text)?;
                conn.poly_fill_rectangle(
                    self.window,
                    self.gc,
                    &[Rectangle {
                        x: x_offset,
                        y: 0,
                        width,
                        height: self.height,
                    }],
                )?;

                let inv_gc = conn.generate_id()?;
                conn.create_gc(
                    inv_gc,
                    self.window,
                    &CreateGCAux::new()
                        .foreground(0x000000) // Black
                        .background(0xFFFFFF) // White
                        .font(self.font),
                )?;
                self.draw_text_gc(conn, inv_gc, x_offset, 14, display_text)?;
                conn.free_gc(inv_gc)?;
            } else {
                self.draw_text(conn, x_offset, 14, display_text)?;
            }

            let w = self.text_width(conn, display_text)?;
            x_offset += w as i16 + 10;
        }

        // 3. Draw Layout
        self.draw_text(conn, x_offset, 14, layout_name)?;
        let layout_w = self.text_width(conn, layout_name)?;
        x_offset += layout_w as i16 + 15;

        // 4. Draw Window Title & Icon
        if let Some(win) = focused_window {
            // A. Draw Icon (Attempt)
            let icon_atom = conn.intern_atom(false, b"_NET_WM_ICON")?.reply()?.atom;
            let icon_reply = conn
                .get_property(false, win, icon_atom, AtomEnum::CARDINAL, 0, 4096)?
                .reply();

            if let Ok(reply) = icon_reply {
                if let Some(data) = reply.value32() {
                    let data: Vec<u32> = data.collect();
                    if data.len() > 2 {
                        let _w = data[0];
                        let _h = data[1];
                        // Placeholder for icon drawing logic
                    }
                }
            }

            // B. Draw Title
            let wm_name = conn
                .get_property(false, win, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 1024)?
                .reply();
            if let Ok(prop) = wm_name {
                let title = String::from_utf8_lossy(&prop.value).to_string();
                let title_w = self.text_width(conn, &title)?;
                let center_x = (self.width as i16 / 2) - (title_w as i16 / 2);
                if center_x > x_offset {
                    self.draw_text(conn, center_x, 14, &title)?;
                }
            }
        }

        // 5. Draw Modules (Right Aligned)
        let mut right_x = self.width as i16 - 10;

        // A. Date & Time
        let time_str = chrono::Local::now().format("%a %b %d  %H:%M").to_string();
        let time_w = self.text_width(conn, &time_str)?;
        right_x -= time_w as i16;
        self.draw_text(conn, right_x, 14, &time_str)?;
        right_x -= 15;

        // B. Script Modules - PHASE 1: UPDATE
        // We iterate by index to update states without holding a borrow on `self` during the command execution
        for i in 0..self.config.modules.len() {
            let interval = self.config.modules[i].interval;
            let elapsed = self.module_states[i].last_update.elapsed();

            if elapsed > Duration::from_secs(interval) {
                let cmd = self.config.modules[i].command.clone();
                // Run command (doesn't borrow self)
                if let Ok(output) = Command::new("sh").arg("-c").arg(&cmd).output() {
                    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    self.module_states[i].last_output = s;
                }
                self.module_states[i].last_update = Instant::now();
            }
        }

        // C. Script Modules - PHASE 2: DRAW
        // Now we can borrow self immutably to draw
        for i in 0..self.config.modules.len() {
            let output = &self.module_states[i].last_output;
            if !output.is_empty() {
                let w = self.text_width(conn, output)?;
                right_x -= w as i16;
                self.draw_text(conn, right_x, 14, output)?;
                right_x -= 15;
            }
        }

        Ok(())
    }

    fn text_width<C: Connection>(
        &self,
        conn: &C,
        text: &str,
    ) -> Result<u16, Box<dyn std::error::Error>> {
        let chars: Vec<Char2b> = text
            .chars()
            .map(|c| {
                let val = c as u16;
                Char2b {
                    byte1: (val >> 8) as u8,
                    byte2: (val & 0xFF) as u8,
                }
            })
            .collect();
        let reply = conn.query_text_extents(self.font, &chars)?.reply()?;
        Ok(reply.overall_width as u16)
    }

    fn draw_text<C: Connection>(
        &self,
        conn: &C,
        x: i16,
        y: i16,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.draw_text_gc(conn, self.gc, x, y, text)
    }

    fn draw_text_gc<C: Connection>(
        &self,
        conn: &C,
        gc: Gcontext,
        x: i16,
        y: i16,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let chars: Vec<Char2b> = text
            .chars()
            .map(|c| {
                let val = c as u16;
                Char2b {
                    byte1: (val >> 8) as u8,
                    byte2: (val & 0xFF) as u8,
                }
            })
            .collect();
        conn.image_text16(self.window, gc, x, y, &chars)?.check()?;
        Ok(())
    }

    pub fn get_clicked_workspace(&self, x: i16) -> Option<usize> {
        let start_x = 10;
        let item_width = 25;
        if x < start_x {
            return None;
        }
        let index = (x - start_x) / item_width;
        if index >= 0 && index < 9 {
            Some(index as usize)
        } else {
            None
        }
    }
}
