use std::collections::VecDeque;
use std::fmt::Write;

use anyhow::*;
use hv_gui::{
    console::{CodeTheme, ConsoleWidget},
    egui,
};
use hv_lua::prelude::*;
use hv_resources::Resources;
use hv_script::ScriptContext;

#[derive(Debug)]
pub struct Console {
    theme: CodeTheme,
    frame: bool,
    hint_text: String,

    desired_width: Option<f32>,
    desired_history_rows: usize,
    desired_buffer_rows: usize,

    output: String,
    buffer_index: usize,
    buffers: VecDeque<String>,
    front_dirty: bool,

    // how deep are past commands?
    history_depth: usize,

    // Commands which have been submitted and not yet run.
    submitted: Vec<String>,
}

impl Console {
    pub fn new_widget() -> Self {
        Self {
            theme: CodeTheme::dark(),
            frame: true,
            hint_text: "> Enter commands (Shift-Enter to run)".to_owned(),

            desired_width: None,
            desired_history_rows: 11,
            desired_buffer_rows: 1,

            output: String::new(),
            buffer_index: 0,
            buffers: VecDeque::new(),
            front_dirty: false,
            history_depth: 0,
            submitted: Vec::new(),
        }
    }

    pub fn new_overlay() -> Self {
        Self {
            theme: CodeTheme::dark(),
            frame: false,
            hint_text: "> Enter commands (Shift-Enter to run)".to_owned(),

            desired_width: Some(f32::INFINITY),
            desired_history_rows: 11,
            desired_buffer_rows: 1,

            output: String::new(),
            buffer_index: 0,
            buffers: VecDeque::new(),
            front_dirty: false,
            history_depth: 0,
            submitted: Vec::new(),
        }
    }

    /// Show the console as a whole-screen overlay, with no background - just text on whatever's
    /// rendered behind it.
    pub fn show_overlay(&mut self, ctx: &egui::CtxRef) {
        let available_rect = ctx.available_rect();
        let layer_id = egui::LayerId::background();
        let id = egui::Id::new("boneless_console");

        let clip_rect = ctx.input().screen_rect();
        let mut panel_ui = egui::Ui::new(ctx.clone(), layer_id, id, available_rect, clip_rect);

        let panel_rect = panel_ui.available_rect_before_wrap();
        let mut panel_ui = panel_ui.child_ui(panel_rect, egui::Layout::bottom_up(egui::Align::Min));

        // ctx.debug_painter()
        //     .debug_rect(panel_rect, Color32::RED, "panel_rect");

        panel_ui.expand_to_include_rect(panel_ui.max_rect()); // Expand frame to include it all
        self.show_widget(&mut panel_ui)

        // Only inform ctx about what we actually used, so we can shrink the native window to fit.
        // ctx.frame_state()
        //     .allocate_central_panel(inner_response.response.rect);
    }

    /// Show the console as a widget - place this in an Egui `Window` or `Area` or `Panel` or
    /// whatever. If you want a whole-screen overlay w/ no background, use `show_overlay`.
    pub fn show_widget(&mut self, ui: &mut egui::Ui) {
        if self.buffers.is_empty() {
            // always wanna have at least one in the chamber.
            self.buffers.push_front(String::new());
        }

        let mut buffer_index_out = self.buffer_index;
        let mut layouter = hv_gui::console::syntax_highlighter(&self.theme, "lua");

        let mut c = ConsoleWidget::new(
            &mut self.output,
            &mut buffer_index_out,
            &mut self.buffers[0],
        )
        .frame(self.frame)
        .layouter(&mut layouter)
        .hint_text(&self.hint_text)
        .desired_buffer_rows(self.desired_buffer_rows)
        .desired_history_rows(self.desired_history_rows);

        if let Some(width) = self.desired_width {
            c = c.desired_width(width);
        }

        let widget_out = c.show(ui);

        if widget_out.inner.buffer_response.changed() {
            self.front_dirty = true;
        }

        if buffer_index_out != self.buffer_index && buffer_index_out < self.buffers.len() {
            // the user went back or forward in the buffer queue, and the new index is valid
            if !self.front_dirty {
                // if the top buffer isn't dirty, we need to copy whatever index we're trying to
                // go to, to index 0.
                //
                // sadly i can't think of a more efficient way to do this right now.
                let copy = self.buffers[buffer_index_out].clone();
                self.buffers[0].clone_from(&copy);
                self.buffer_index = buffer_index_out;
            } else {
                // clone that buffer, and push it to the front of the buffer queue (index 0)
                let copy = self.buffers[buffer_index_out].clone();
                self.buffers.push_front(copy);
                self.front_dirty = false;

                // history is now one deeper than before...
                self.history_depth += 1;
                // and so is the buffer index.
                self.buffer_index = buffer_index_out + 1;
            }
        }

        if let Some(command) = widget_out.inner.submitted {
            // the user submitted a command. as the user is always editing the front of the queue,d
            // we don't have to worry about correcting history. just push a blank buffer onto the
            // front, and push the submitted command into the submitted queue.
            self.buffers.drain(1..self.history_depth + 1);

            if Some(&command) == self.buffers.get(1) {
                self.buffers[0].clear();
            } else if !command.is_empty() {
                self.buffers.push_front(String::new());
            }

            self.buffer_index = 0;
            self.front_dirty = false;
            self.history_depth = 0;
            self.submitted.push(command);
        }
    }

    pub fn process(
        &mut self,
        lua: &Lua,
        script_context: &mut ScriptContext,
        resources: &Resources,
    ) -> Result<()> {
        script_context.with_resources(lua, resources, |env| {
            for command in self.submitted.drain(..) {
                write!(&mut self.output, "\n> {}", command.trim_end()).unwrap();

                let out = lua
                    .load(&command)
                    .set_environment(env.clone())
                    .and_then(|chunk| chunk.eval::<LuaMultiValue>())
                    .and_then(|result| {
                        if result.is_empty() {
                            Ok(vec![])
                        } else {
                            result
                                .into_iter()
                                .map(|value| lua.globals().call_function("tostring", value))
                                .collect::<Result<Vec<String>, _>>()
                        }
                    });

                let out_strings = match out {
                    Ok(results) => results,
                    Err(error) => vec![error.to_string()],
                };

                for (i, out_string) in out_strings.into_iter().enumerate() {
                    writeln!(
                        &mut self.output,
                        "\nOut[{}]: {}",
                        i + 1,
                        out_string.trim_end(),
                    )
                    .unwrap();
                }
            }
        })
    }
}
