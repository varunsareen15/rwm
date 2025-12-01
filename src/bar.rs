use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    ConfigureWindowAux, ConnectionExt, CreateWindowAux, Gcontext, Rectangle, Screen, StackMode,
    Window, WindowClass,
};

pub struct Bar {
    pub window: Window,
    gc: Gcontext,
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
        let height = 20;
        let width = screen.width_in_pixels;

        // Create the bar
        let win_aux = CreateWindowAux::new()
            .background_pixel(screen.black_pixel)
            .override_redirect(1) // Tell WM to ignore this window
            .event_mask(x11rb::protocol::xproto::EventMask::EXPOSURE);

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
            .graphics_exposures(0);

        conn.create_gc(gc, window, &gc_aux)?;

        // Show the bar
        conn.map_window(window)?;

        Ok(Self {
            window,
            gc,
            width,
            height,
        })
    }

    pub fn draw<C: Connection>(
        &self,
        conn: &C,
        active_idx: usize,
        total_workspaces: usize,
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

        Ok(())
    }
}
