use crate::layout::Layout;
use x11rb::protocol::xproto::Window;

pub struct Workspace {
    pub windows: Vec<Window>,
    pub layout: Layout,
}

impl Workspace {
    pub fn new() -> Self {
        Self {
            windows: Vec::new(),
            layout: Layout::MasterStack, // Default layout
        }
    }
}
