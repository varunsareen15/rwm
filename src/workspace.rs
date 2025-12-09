use crate::layout::Layout;
use x11rb::protocol::xproto::Window;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SplitAxis {
    Horizontal,
    Vertical,
}

pub struct Workspace {
    pub windows: Vec<Window>,
    pub layout: Layout,
    pub split_history: Vec<SplitAxis>,
}

impl Workspace {
    pub fn new() -> Self {
        Self {
            windows: Vec::new(),
            layout: Layout::MasterStack, // Default layout
            split_history: Vec::new(),
        }
    }
}
