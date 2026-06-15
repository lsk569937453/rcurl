/// Application state for the TUI interface
pub struct App {
    pub should_exit: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            should_exit: false,
        }
    }
}
