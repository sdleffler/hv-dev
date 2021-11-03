#[derive(Debug, Clone, Copy)]
pub enum WindowKind {
    Windowed { width: u32, height: u32 },
    Fullscreen,
    FullscreenRestricted { width: u32, height: u32 },
}

#[cfg(feature = "glfw")]
impl From<WindowKind> for luminance_windowing::WindowDim {
    fn from(kind: WindowKind) -> Self {
        use luminance_windowing::WindowDim;

        match kind {
            WindowKind::Windowed { width, height } => WindowDim::Windowed { width, height },
            WindowKind::Fullscreen => WindowDim::Fullscreen,
            WindowKind::FullscreenRestricted { width, height } => {
                WindowDim::FullscreenRestricted { width, height }
            }
        }
    }
}
