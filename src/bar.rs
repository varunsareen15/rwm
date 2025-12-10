use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    Char2b, ConfigureWindowAux, ConnectionExt, CreateWindowAux, EventMask, Font, Gcontext,
    Rectangle, Screen, StackMode, Window, WindowClass,
};

pub struct Bar {
    pub window: Window,
    gc: Gcontext,
    font: Font,
    width: u16,
    height: u16,
}

impl Bar {
    pub fn new<C: Connection>(
        conn: &C,
        screen: &Screen,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let window = conn.generate_id()?;
        let gc = conn.generate_id()?;
        let font = conn.generate_id()?;
        let height = 20;
        let width = screen.width_in_pixels;

        conn.open_font(font, b"fixed")?;

        // Create the bar
        let win_aux = CreateWindowAux::new()
            .background_pixel(screen.black_pixel)
            .override_redirect(1) // Tell WM to ignore this window
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

        // Create a Graphics Context (Pen) for drawing
        let gc_aux = x11rb::protocol::xproto::CreateGCAux::new()
            .foreground(screen.white_pixel)
            .background(screen.black_pixel)
            .font(font)
            .graphics_exposures(0);

        conn.create_gc(gc, window, &gc_aux)?;

        // Show the bar
        conn.map_window(window)?;

        Ok(Self {
            window,
            gc,
            font,
            width,
            height,
        })
    }

    pub fn draw<C: Connection>(
        &self,
        conn: &C,
        active_idx: usize,
        total_workspaces: usize,
        layout_name: &str,
        window_title: &str,
        clock: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let stack = ConfigureWindowAux::new().stack_mode(StackMode::ABOVE);
        conn.configure_window(self.window, &stack)?;
        conn.clear_area(false, self.window, 0, 0, self.width, self.height)?;

        // Draw the blocks
        let block_size = 14;
        let gap = 4;
        let start_x = 10;
        let start_y = 3;

        let mut rects = Vec::new();
        let mut filled_rects = Vec::new();

        for i in 0..total_workspaces {
            let x = start_x + (i as i16 * (block_size + gap));
            let rect = Rectangle {
                x,
                y: start_y,
                width: block_size as u16,
                height: block_size as u16,
            };

            if i == active_idx {
                filled_rects.push(rect);
            } else {
                rects.push(rect);
            }
        }

        // Draw outlines
        if !rects.is_empty() {
            conn.poly_rectangle(self.window, self.gc, &rects)?;
        }

        if !filled_rects.is_empty() {
            conn.poly_fill_rectangle(self.window, self.gc, &filled_rects)?;
        }

        let ws_end_x = start_x + (total_workspaces as i16 * (block_size + gap));
        let layout_x = ws_end_x + 10;
        self.draw_text(conn, layout_x, 14, layout_name)?;

        if !window_title.is_empty() {
            let title_w = self.text_width(conn, window_title)?;
            let center_x = (self.width as i16 / 2) - (title_w as i16 / 2);
            self.draw_text(conn, center_x, 14, window_title)?;
        }

        if !clock.is_empty() {
            let clock_w = self.text_width(conn, clock)?;
            let right_w = (self.width as i16) - (clock_w as i16) - 10;
            self.draw_text(conn, right_w, 14, clock)?;
        }

        Ok(())
    }

    fn text_width<C: Connection>(
        &self,
        conn: &C,
        text: &str,
    ) -> Result<u16, Box<dyn std::error::Error>> {
        let chars: Vec<Char2b> = text
            .as_bytes()
            .iter()
            .map(|&b| Char2b { byte1: 0, byte2: b })
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
        let bytes = text.as_bytes();
        conn.image_text8(self.window, self.gc, x, y, bytes)?
            .check()?;
        Ok(())
    }

    pub fn get_clicked_workspace(&self, x: i16) -> Option<usize> {
        let block_size = 14;
        let gap = 4;
        let start_x = 10;

        if x < start_x {
            return None;
        }

        let index = (x - start_x) / (block_size + gap);

        if index >= 0 && index < 9 {
            Some(index as usize)
        } else {
            None
        }
    }
}
