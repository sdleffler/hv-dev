//! A Lua console widget for Egui through `hv-gui`, based on the `egui::TextEdit` type.

mod builder;
mod cursor_range;
mod output;
mod state;
mod text_buffer;

pub use {
    builder::Console, cursor_range::*, output::ConsoleOutput, state::ConsoleState,
    text_buffer::TextBuffer,
};
