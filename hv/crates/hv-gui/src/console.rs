//! A Lua console widget for Egui through `hv-gui`, based on the `egui::TextEdit` type.

use std::sync::Arc;

use egui::Galley;

mod builder;
mod cursor_range;
mod output;
mod state;
mod syntax_highlight;
mod text_buffer;

pub use {
    builder::Console, cursor_range::*, output::ConsoleOutput, state::ConsoleState,
    syntax_highlight::CodeTheme, text_buffer::TextBuffer,
};

pub fn syntax_highlighter<'a>(
    theme: &'a CodeTheme,
    language: &'a str,
) -> impl FnMut(&egui::Ui, &str, f32) -> Arc<Galley> + 'a {
    |ui: &egui::Ui, string: &str, wrap_width: f32| {
        let mut layout_job = self::syntax_highlight::highlight(ui.ctx(), theme, string, language);
        layout_job.wrap_width = wrap_width;
        ui.fonts().layout_job(layout_job)
    }
}
