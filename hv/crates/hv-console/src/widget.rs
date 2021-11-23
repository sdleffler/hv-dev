//! A Lua console widget for Egui through `hv-gui`, based on the `egui::TextEdit` type.
//!
//! This is a heavily modified version of the Egui `TextEdit` widget. There are still a lot of
//! references to `TextEdit` throughout as it was gently hacked together with a focus on making a
//! usable in-game console rather than making a nicely documented module. As such, a good pass
//! through the internals is in order, removing references to `TextEdit` and commenting on
//! similarities to `TextEdit` where applicable.
//!
//! In addition, it draws on the "code editor" Egui example for syntax highlighting through syntect,
//! specifically for Lua syntax highlighting. This feels a little heavy duty, but is VERY robust.

use std::sync::Arc;

use egui::Galley;
use hv_gui::egui;

mod builder;
mod cursor_range;
mod output;
mod state;
mod syntax_highlight;
mod text_buffer;

pub use {
    builder::ConsoleWidget, cursor_range::*, output::ConsoleOutput, state::ConsoleState,
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
