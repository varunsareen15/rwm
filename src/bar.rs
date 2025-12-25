use crate::config::BarConfig;
use rusttype::{point, Font, Scale};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ConnectionExt, CreateGCAux, CreateWindowAux, EventMask, Gcontext,
    ImageFormat, Rectangle, Screen, Window, WindowClass,
};
use std::fs;
use std::process::Command;
use std::time::{Instant, Duration};

// --- CONSTANTS ---
const CELL_WIDTH: i16 = 30;

pub struct ModuleState {
    pub last_output: String,
    pub last_update: Instant,
}

pub struct Bar {
    pub window: Window,
    gc: Gcontext,
    width: u16,
    height: u16,
    config: BarConfig,
    module_states: Vec<ModuleState>,
    // Modern Font Data
    font: Option<Font<'static>>,
    font_data: Vec<u8>, // Keep the bytes in memory
}

impl Bar {
    pub fn new<C: Connection>(
        conn: &C,
        screen: &Screen,
        config: BarConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let window = conn.generate_id()?;
        let gc = conn.generate_id()?;
        let height = 24; // Slightly taller for modern fonts
        let width = screen.width_in_pixels;

        // 1. Load Font from File
        let font_path = &config.font;
        let mut font = None;
        let mut font_data = Vec::new();

        // Try to load the TTF/OTF file
        match fs::read(font_path) {
            Ok(data) => {
                font_data = data;
                // We must use 'unsafe' to cast the reference lifetime, or clone the data.
                // Since 'Bar' owns 'font_data', it's safe to reference it as long as Bar exists.
                // To keep it safe Rust, we construct the Font from the slice every time OR 
                // use 'try_from_vec' if available, but rusttype usually takes a slice.
                // Hack: We re-parse the font from the owned vector.
                if let Some(f) = Font::try_from_vec(font_data.clone()) {
                     font = Some(f);
                } else {
                    log::error!("Failed to parse font file: {}", font_path);
                }
            },
            Err(e) => log::error!("Could not read font file '{}': {}", font_path, e),
        }

        // 2. Create Window
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

        // 3. Create GC
        let gc_aux = CreateGCAux::new()
            .foreground(screen.white_pixel)
            .background(screen.black_pixel)
            .graphics_exposures(0);

        conn.create_gc(gc, window, &gc_aux)?;
        conn.map_window(window)?;

        let module_states = config.modules.iter().map(|_| ModuleState {
            last_output: String::new(),
            last_update: Instant::now() - Duration::from_secs(100),
        }).collect();

        Ok(Self {
            window,
            gc,
            width,
            height,
            config,
            module_states,
            font,
            font_data,
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
        // Clear Bar
        conn.clear_area(false, self.window, 0, 0, self.width, self.height)?;

        let mut x_offset = 0i16;
        let bg_color = 0x000000; // Black
        let fg_color = 0xFFFFFF; // White
        let active_bg = 0xFFFFFF; // White
        let active_fg = 0x000000; // Black

        // 1. Draw Workspaces
        for (i, icon) in self.config.workspace_icons.iter().enumerate() {
            let is_active = i == active_idx;
            let cell_x = i as i16 * CELL_WIDTH;
            
            // Text to draw
            let display_text = if self.config.workspace_style == "Squares" {
                if is_active { "[x]" } else { "[ ]" }
            } else {
                icon.as_str()
            };

            // Measure Text
            let text_w = self.measure_text(display_text) as i16;
            let center_x = cell_x + (CELL_WIDTH - text_w) / 2;
            // Vertically center: (Bar Height / 2) + (Font Height / 4 approx)
            let center_y = (self.height as f32 / 2.0) + 4.0; 

            if is_active {
                // Draw Active Background
                conn.poly_fill_rectangle(self.window, self.gc, &[Rectangle{
                    x: cell_x, y: 0, width: CELL_WIDTH as u16, height: self.height
                }])?;
                
                // Draw Text (Inverted)
                self.draw_text_modern(conn, center_x, center_y as i16, display_text, active_fg, active_bg)?;
            } else {
                // Draw Inactive Text
                self.draw_text_modern(conn, center_x, center_y as i16, display_text, fg_color, bg_color)?;
            }
        }

        x_offset = (self.config.workspace_icons.len() as i16 * CELL_WIDTH) + 10;

        // 2. Draw Layout Symbol
        self.draw_text_modern(conn, x_offset, ((self.height/2)+4) as i16, layout_name, fg_color, bg_color)?;
        let layout_w = self.measure_text(layout_name) as i16;
        x_offset += layout_w + 15;

        // 3. Draw Window Title
        if let Some(win) = focused_window {
            let wm_name = conn.get_property(false, win, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 1024)?.reply();
            if let Ok(prop) = wm_name {
                 let title = String::from_utf8_lossy(&prop.value).to_string();
                 let title_w = self.measure_text(&title) as i16;
                 
                 let center_x = (self.width as i16 / 2) - (title_w / 2);
                 if center_x > x_offset {
                     self.draw_text_modern(conn, center_x, ((self.height/2)+4) as i16, &title, fg_color, bg_color)?;
                 }
            }
        }

        // 4. Draw Modules
        let mut right_x = self.width as i16 - 10;

        // A. Time
        let time_str = chrono::Local::now().format("%a %b %d  %H:%M").to_string();
        let time_w = self.measure_text(&time_str) as i16;
        right_x -= time_w;
        self.draw_text_modern(conn, right_x, ((self.height/2)+4) as i16, &time_str, fg_color, bg_color)?;
        right_x -= 15;

        // B. Update & Draw Modules
        for i in 0..self.config.modules.len() {
             // Update
             let interval = self.config.modules[i].interval;
             if self.module_states[i].last_update.elapsed() > Duration::from_secs(interval) {
                let cmd = self.config.modules[i].command.clone();
                if let Ok(output) = Command::new("sh").arg("-c").arg(&cmd).output() {
                    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    self.module_states[i].last_output = s;
                }
                self.module_states[i].last_update = Instant::now();
             }

             // Draw
             let output = &self.module_states[i].last_output;
             if !output.is_empty() {
                let w = self.measure_text(output) as i16;
                right_x -= w;
                self.draw_text_modern(conn, right_x, ((self.height/2)+4) as i16, output, fg_color, bg_color)?;
                right_x -= 15;
             }
        }

        Ok(())
    }

    // --- MODERN TEXT RENDERING ---

    fn measure_text(&self, text: &str) -> u32 {
        if let Some(font) = &self.font {
            let scale = Scale::uniform(16.0); // 16px Font Size
            let v_metrics = font.v_metrics(scale);
            
            let mut width = 0.0;
            for glyph in font.layout(text, scale, point(0.0, v_metrics.ascent)) {
                if let Some(bb) = glyph.pixel_bounding_box() {
                    width = bb.max.x as f32;
                }
            }
            return width as u32;
        }
        // Fallback estimate
        (text.len() * 8) as u32
    }

    fn draw_text_modern<C: Connection>(
        &self, 
        conn: &C, 
        x: i16, 
        y: i16, 
        text: &str, 
        text_color: u32,
        bg_color: u32
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(font) = &self.font {
            let scale = Scale::uniform(16.0); // Font Size
            let v_metrics = font.v_metrics(scale);
            
            // 1. Calculate dimensions
            let width = self.measure_text(text) as usize;
            let height = 24; // Bar height
            
            if width == 0 { return Ok(()); }

            // 2. Create Pixel Buffer (ARGB or BGRA usually)
            // We initialize with the background color
            let mut pixel_buffer = vec![0u8; width * height * 4];
            
            for i in 0..(width * height) {
                // Fill with BG color
                let b = (bg_color & 0xFF) as u8;
                let g = ((bg_color >> 8) & 0xFF) as u8;
                let r = ((bg_color >> 16) & 0xFF) as u8;
                let a = 0xFF; // Full opacity
                
                pixel_buffer[i * 4 + 0] = b;
                pixel_buffer[i * 4 + 1] = g;
                pixel_buffer[i * 4 + 2] = r;
                pixel_buffer[i * 4 + 3] = a;
            }

            // 3. Render Glyphs
            // We render starting at (0, baseline) relative to our buffer
            let offset = point(0.0, v_metrics.ascent);

            for glyph in font.layout(text, scale, offset) {
                if let Some(bb) = glyph.pixel_bounding_box() {
                    glyph.draw(|gx, gy, v| {
                        // v is coverage (alpha) from 0.0 to 1.0
                        let alpha = v;
                        
                        // Buffer Coordinates
                        let px = (bb.min.x + gx as i32) as usize;
                        let py = (bb.min.y + gy as i32) as usize;

                        // Check bounds (important!)
                        if px < width && py < height {
                            let idx = (py * width + px) * 4;
                            
                            // Get existing color (Background)
                            let bg_b = pixel_buffer[idx + 0] as f32;
                            let bg_g = pixel_buffer[idx + 1] as f32;
                            let bg_r = pixel_buffer[idx + 2] as f32;
                            
                            // Get text color
                            let fg_b = (text_color & 0xFF) as f32;
                            let fg_g = ((text_color >> 8) & 0xFF) as f32;
                            let fg_r = ((text_color >> 16) & 0xFF) as f32;

                            // Alpha Blend: Out = Alpha * FG + (1-Alpha) * BG
                            let out_b = (alpha * fg_b + (1.0 - alpha) * bg_b) as u8;
                            let out_g = (alpha * fg_g + (1.0 - alpha) * bg_g) as u8;
                            let out_r = (alpha * fg_r + (1.0 - alpha) * bg_r) as u8;

                            pixel_buffer[idx + 0] = out_b;
                            pixel_buffer[idx + 1] = out_g;
                            pixel_buffer[idx + 2] = out_r;
                            // Alpha stays 0xFF
                        }
                    });
                }
            }

            // 4. Send Image to X Server
            conn.put_image(
                ImageFormat::Z_PIXMAP,
                self.window,
                self.gc,
                width as u16,
                height as u16,
                x,
                y - (v_metrics.ascent as i16), // Adjust Y back to top-left of rect
                0,
                24, // Depth (Check your screen.root_depth!)
                &pixel_buffer
            )?;

        } else {
            // Fallback for no font loaded
        }
        Ok(())
    }

    pub fn get_clicked_workspace(&self, x: i16) -> Option<usize> {
        if x < 0 { return None; }
        let index = x / CELL_WIDTH;
        if index >= 0 && index < 9 { Some(index as usize) } else { None }
    }
}
