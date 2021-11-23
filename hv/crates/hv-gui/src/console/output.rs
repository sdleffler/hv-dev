use std::sync::Arc;

/// The output from a `TextEdit`.
pub struct ConsoleOutput {
    /// The interaction response just on the history buffer.
    pub history_response: egui::Response,

    /// The interaction response just on the edit buffer.
    pub buffer_response: egui::Response,

    /// How the "history" text was displayed.
    pub history_galley: Arc<egui::Galley>,

    /// How the edit buffer text was displayed.
    pub buffer_galley: Arc<egui::Galley>,

    /// The state we stored after the run.
    pub state: super::ConsoleState,

    /// Where the text cursor is.
    pub cursor_range: Option<super::state::Section<super::CursorRange>>,

    /// The submitted command, if any.
    pub submitted: Option<String>,
}
