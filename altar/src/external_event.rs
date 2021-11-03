use hv::{
    input::{InputEvent, Mappable},
    prelude::{Point2, Vector2},
};

/// Possible events external to the framework.
#[derive(Debug, Clone, Copy)]
pub enum ExternalEvent<Axes, Buttons>
where
    Axes: Mappable,
    Buttons: Mappable,
{
    /// An input event mapped to the parameterized logical axes/buttons.
    Mapped(InputEvent<Axes, Buttons>),
    /// An input event corresponding to a user's attempt to input text. Unlike a character input
    /// this is processed w/ whatever modifiers, input adapters, etc. are external to the program.
    Text(char),
    /// Notification of a change in content scale (for example a window being moved from a low-DPI
    /// to a high-DPI monitor, or vice versa) if applicable.
    ContentScale(Vector2<f32>),
    /// Notification of a change in the backbuffer size.
    FramebufferSize(Vector2<u32>),
    /// Notification of the cursor entering (true) or leaving (false) the window, if applicable.
    WindowCursorEnter(bool),
    /// Notification of acquisition or loss of focus.
    WindowFocus(bool),
    /// Notification of a change in the position of the window, if applicable.
    WindowPos(Point2<u32>),
    /// Notification of a change in the size of the window, if applicable.
    WindowSize(Vector2<u32>),
    /// Notification of minimization (being "iconified.")
    WindowMinimize(bool),
    /// Notification of maximization.
    WindowMaximize(bool),
    /// Notification that the window contents have been "damaged" and need to be refreshed.
    WindowRefresh,
}
