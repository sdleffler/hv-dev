#[derive(Debug, Clone, Copy)]
pub enum WindowKind {
    Windowed { width: u32, height: u32 },
    Fullscreen { width: u32, height: u32 },
}
