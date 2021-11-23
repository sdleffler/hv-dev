use std::sync::Arc;

use egui::epaint::text::{cursor::*, Galley, LayoutJob};
use egui::{output::OutputEvent, *};

use super::{state::Section, CCursorRange, ConsoleOutput, ConsoleState, CursorRange};

/// A text region that the user can edit the contents of.
///
/// See also [`Ui::text_edit_singleline`] and  [`Ui::text_edit_multiline`].
///
/// Example:
///
/// ```
/// # egui::__run_test_ui(|ui| {
/// # let mut my_string = String::new();
/// let response = ui.add(egui::TextEdit::singleline(&mut my_string));
/// if response.changed() {
///     // …
/// }
/// if response.lost_focus() && ui.input().key_pressed(egui::Key::Enter) {
///     // …
/// }
/// # });
/// ```
///
/// To fill an [`Ui`] with a [`TextEdit`] use [`Ui::add_sized`]:
///
/// ```
/// # egui::__run_test_ui(|ui| {
/// # let mut my_string = String::new();
/// ui.add_sized(ui.available_size(), egui::TextEdit::multiline(&mut my_string));
/// # });
/// ```
///
///
/// You can also use [`TextEdit`] to show text that can be selected, but not edited.
/// To do so, pass in a `&mut` reference to a `&str`, for instance:
///
/// ```
/// fn selectable_text(ui: &mut egui::Ui, mut text: &str) {
///     ui.add(egui::TextEdit::multiline(&mut text));
/// }
/// ```
///
#[must_use = "You should put this widget in an ui with `ui.add(widget);`"]
pub struct ConsoleWidget<'t> {
    history: &'t mut dyn TextBuffer,
    history_index: &'t mut usize,
    buffer: &'t mut dyn TextBuffer,
    hint_text: WidgetText,
    id: Option<Id>,
    id_source: Option<Id>,
    text_style: Option<TextStyle>,
    text_color: Option<Color32>,
    layouter: Option<&'t mut dyn FnMut(&Ui, &str, f32) -> Arc<Galley>>,
    frame: bool,
    interactive: bool,
    desired_width: Option<f32>,
    desired_history_height_rows: usize,
    desired_buffer_height_rows: usize,
    lock_focus: bool,
    cursor_at_end: bool,
}

impl<'t> WidgetWithState for ConsoleWidget<'t> {
    type State = ConsoleState;
}

impl<'t> ConsoleWidget<'t> {
    pub fn load_state(ctx: &Context, id: Id) -> Option<ConsoleState> {
        ConsoleState::load(ctx, id)
    }

    pub fn store_state(ctx: &Context, id: Id, state: ConsoleState) {
        state.store(ctx, id);
    }
}

impl<'t> ConsoleWidget<'t> {
    /// A REPL/console which accepts
    pub fn new(
        history: &'t mut dyn TextBuffer,
        history_index: &'t mut usize,
        buffer: &'t mut dyn TextBuffer,
    ) -> Self {
        Self {
            history,
            history_index,
            buffer,
            hint_text: Default::default(),
            id: None,
            id_source: None,
            text_style: Some(TextStyle::Monospace),
            text_color: None,
            layouter: None,
            frame: true,
            interactive: true,
            desired_width: None,
            desired_history_height_rows: 8,
            desired_buffer_height_rows: 3,
            lock_focus: true,
            cursor_at_end: true,
        }
    }

    /// Use if you want to set an explicit `Id` for this widget.
    pub fn id(mut self, id: Id) -> Self {
        self.id = Some(id);
        self
    }

    /// A source for the unique `Id`, e.g. `.id_source("second_text_edit_field")` or `.id_source(loop_index)`.
    pub fn id_source(mut self, id_source: impl std::hash::Hash) -> Self {
        self.id_source = Some(Id::new(id_source));
        self
    }

    /// Show a faint hint text when the text field is empty.
    pub fn hint_text(mut self, hint_text: impl Into<WidgetText>) -> Self {
        self.hint_text = hint_text.into();
        self
    }

    pub fn text_style(mut self, text_style: TextStyle) -> Self {
        self.text_style = Some(text_style);
        self
    }

    pub fn text_color(mut self, text_color: Color32) -> Self {
        self.text_color = Some(text_color);
        self
    }

    pub fn text_color_opt(mut self, text_color: Option<Color32>) -> Self {
        self.text_color = text_color;
        self
    }

    /// Override how text is being shown inside the `TextEdit`.
    ///
    /// This can be used to implement things like syntax highlighting.
    ///
    /// This function will be called at least once per frame,
    /// so it is strongly suggested that you cache the results of any syntax highlighter
    /// so as not to waste CPU highlighting the same string every frame.
    ///
    /// The arguments is the enclosing [`Ui`] (so you can access e.g. [`Ui::fonts`]),
    /// the text and the wrap width.
    ///
    /// ```
    /// # egui::__run_test_ui(|ui| {
    /// # let mut my_code = String::new();
    /// # fn my_memoized_highlighter(s: &str) -> egui::text::LayoutJob { Default::default() }
    /// let mut layouter = |ui: &egui::Ui, string: &str, wrap_width: f32| {
    ///     let mut layout_job: egui::text::LayoutJob = my_memoized_highlighter(string);
    ///     layout_job.wrap_width = wrap_width;
    ///     ui.fonts().layout_job(layout_job)
    /// };
    /// ui.add(egui::TextEdit::multiline(&mut my_code).layouter(&mut layouter));
    /// # });
    /// ```
    pub fn layouter(mut self, layouter: &'t mut dyn FnMut(&Ui, &str, f32) -> Arc<Galley>) -> Self {
        self.layouter = Some(layouter);

        self
    }

    /// Default is `true`. If set to `false` then you cannot interact with the text (neither edit or select it).
    ///
    /// Consider using [`Ui::add_enabled`] instead to also give the `TextEdit` a greyed out look.
    pub fn interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self
    }

    /// Default is `true`. If set to `false` there will be no frame showing that this is editable text!
    pub fn frame(mut self, frame: bool) -> Self {
        self.frame = frame;
        self
    }

    /// Set to 0.0 to keep as small as possible.
    /// Set to [`f32::INFINITY`] to take up all available space (i.e. disable automatic word wrap).
    pub fn desired_width(mut self, desired_width: f32) -> Self {
        self.desired_width = Some(desired_width);
        self
    }

    /// Set the number of rows of history to show by default.
    /// The default is `8`.
    pub fn desired_history_rows(mut self, desired_height_rows: usize) -> Self {
        self.desired_history_height_rows = desired_height_rows;
        self
    }

    /// Set the number of rows of the edit buffer to show by default.
    /// The default is `3`.
    pub fn desired_buffer_rows(mut self, desired_height_rows: usize) -> Self {
        self.desired_buffer_height_rows = desired_height_rows;
        self
    }

    /// When `false`, pressing TAB will move focus
    /// to the next widget.
    ///
    /// When `true` (default), the widget will keep the focus and pressing TAB
    /// will insert the `'\t'` character.
    pub fn lock_focus(mut self, b: bool) -> Self {
        self.lock_focus = b;
        self
    }

    /// When `true` (default), the cursor will initially be placed at the end of the text.
    ///
    /// When `false`, the cursor will initially be placed at the beginning of the text.
    pub fn cursor_at_end(mut self, b: bool) -> Self {
        self.cursor_at_end = b;
        self
    }
}

// ----------------------------------------------------------------------------

impl<'t> Widget for ConsoleWidget<'t> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui).response
    }
}

impl<'t> ConsoleWidget<'t> {
    /// Show the [`Console`], returning a rich [`ConsoleOutput`].
    pub fn show(self, ui: &mut Ui) -> InnerResponse<ConsoleOutput> {
        let is_mutable = self.buffer.is_mutable();
        let frame = self.frame;
        let interactive = self.interactive;
        let where_to_put_background = ui.painter().add(Shape::Noop);

        let margin = Vec2::new(4.0, 2.0);
        let max_rect = ui.available_rect_before_wrap().shrink2(margin);
        let mut content_ui = ui.child_ui(max_rect, Layout::bottom_up(Align::Min));
        let mut output = self.show_content(&mut content_ui);
        let id = output.response.id;
        let buffer_id = output.inner.buffer_response.id;
        let frame_rect = output.response.rect; //.expand2(margin);
        ui.allocate_space(frame_rect.size());
        if interactive {
            output.response |= ui.interact(frame_rect, id, Sense::click());
        }
        if output.inner.buffer_response.clicked() && !output.inner.buffer_response.lost_focus() {
            ui.memory().request_focus(buffer_id);
        }

        if frame {
            let visuals = ui.style().interact(&output.response);
            let frame_rect = frame_rect.expand(visuals.expansion);
            let shape = if is_mutable {
                if output.response.has_focus() {
                    epaint::RectShape {
                        rect: frame_rect,
                        corner_radius: visuals.corner_radius,
                        // fill: ui.visuals().selection.bg_fill,
                        fill: ui.visuals().extreme_bg_color,
                        stroke: ui.visuals().selection.stroke,
                    }
                } else {
                    epaint::RectShape {
                        rect: frame_rect,
                        corner_radius: visuals.corner_radius,
                        fill: ui.visuals().extreme_bg_color,
                        stroke: visuals.bg_stroke, // TODO: we want to show something here, or a text-edit field doesn't "pop".
                    }
                }
            } else {
                let visuals = &ui.style().visuals.widgets.inactive;
                epaint::RectShape {
                    rect: frame_rect,
                    corner_radius: visuals.corner_radius,
                    // fill: ui.visuals().extreme_bg_color,
                    // fill: visuals.bg_fill,
                    fill: Color32::TRANSPARENT,
                    stroke: visuals.bg_stroke, // TODO: we want to show something here, or a text-edit field doesn't "pop".
                }
            };

            ui.painter().set(where_to_put_background, shape);
        }

        output
    }

    fn show_content(self, ui: &mut Ui) -> InnerResponse<ConsoleOutput> {
        let ConsoleWidget {
            history,
            history_index,
            buffer,
            hint_text,
            id,
            id_source,
            text_style,
            text_color,
            layouter,
            frame: _,
            interactive,
            desired_width,
            desired_history_height_rows,
            desired_buffer_height_rows,
            lock_focus,
            cursor_at_end,
        } = self;

        let text_color = text_color
            .or(ui.visuals().override_text_color)
            // .unwrap_or_else(|| ui.style().interact(&response).text_color()); // too bright
            .unwrap_or_else(|| ui.visuals().widgets.inactive.text_color());

        let prev_text = buffer.as_ref().to_owned();
        let text_style = text_style
            .or(ui.style().override_text_style)
            .unwrap_or_else(|| ui.style().body_text_style);
        let row_height = ui.fonts().row_height(text_style);
        const MIN_WIDTH: f32 = 24.0; // Never make a `TextEdit` more narrow than this.
        let available_width = ui.available_width().at_least(MIN_WIDTH);
        let desired_width = desired_width.unwrap_or_else(|| ui.spacing().text_edit_width);
        let wrap_width = if ui.layout().horizontal_justify() {
            available_width
        } else {
            desired_width.min(available_width)
        };

        let mut default_layouter = move |ui: &Ui, text: &str, wrap_width: f32| {
            ui.fonts().layout_job(LayoutJob::simple(
                text.to_owned(),
                text_style,
                text_color,
                wrap_width,
            ))
        };

        let layouter = layouter.unwrap_or(&mut default_layouter);

        let mut history_galley = layouter(ui, history.as_ref(), wrap_width);
        let mut buffer_galley = layouter(ui, buffer.as_ref(), wrap_width);

        let desired_history_width = history_galley.size().x.max(wrap_width); // always show everything in multiline
        let desired_history_height = (desired_history_height_rows.at_least(1) as f32) * row_height;
        let desired_history_size = vec2(
            desired_history_width,
            history_galley.size().y.max(desired_history_height),
        );

        let desired_buffer_width = buffer_galley.size().x.max(wrap_width); // always show everything in multiline
        let desired_buffer_height = (desired_buffer_height_rows.at_least(1) as f32) * row_height;
        let desired_buffer_size = vec2(
            desired_buffer_width,
            buffer_galley.size().y.max(desired_buffer_height),
        );

        let total_desired_size = vec2(
            desired_history_size.x.max(desired_buffer_size.x),
            desired_history_size.y + desired_buffer_size.y,
        )
        .min(ui.available_size_before_wrap());

        let add_contents = |ui: &mut Ui| {
            let (history_id, history_rect) = ui.allocate_space(desired_history_size);
            let (buffer_id, buffer_rect) = ui.allocate_space(desired_buffer_size);
            let history_painter = ui.painter_at(history_rect);
            let buffer_painter = ui.painter_at(buffer_rect);

            let id = id.unwrap_or_else(|| {
                if let Some(id_source) = id_source {
                    ui.make_persistent_id(id_source)
                } else {
                    ui.make_persistent_id((history_id, buffer_id)) // Since we are only storing the cursor a persistent Id is not super important
                }
            });
            let mut state = ConsoleState::load(ui.ctx(), id).unwrap_or_default();

            // On touch screens (e.g. mobile in egui_web), should
            // dragging select text, or scroll the enclosing `ScrollArea` (if any)?
            // Since currently copying selected text in not supported on `egui_web`,
            // we prioritize touch-scrolling:
            let buffer_or_history_has_focus = {
                let memory = ui.memory();
                memory.has_focus(buffer_id) || memory.has_focus(history_id)
            };
            let allow_drag_to_select = !ui.input().any_touches() || buffer_or_history_has_focus;

            let sense = if interactive {
                if allow_drag_to_select {
                    Sense::click_and_drag()
                } else {
                    Sense::click()
                }
            } else {
                Sense::hover()
            };

            let mut buffer_response = ui.interact(buffer_rect, buffer_id, sense);
            let mut history_response = ui.interact(history_rect, history_id, sense);
            // let buffer_clip = Rect::from_min_size(
            //     Pos2::new(buffer_rect.min.x, buffer_rect.min.y + viewport.min.y),
            //     buffer_rect.size(),
            // );
            // let buffer_painter = ui.painter_at(buffer_clip);
            // let history_clip = Rect::from_min_size(
            //     Pos2::new(
            //         history_rect.min.x,
            //         buffer_painter.clip_rect().max.y + viewport.min.y,
            //     ),
            //     history_rect.size(),
            // );
            // let history_painter = ui.painter_at(history_clip);

            // {
            //     let mut p = ui.painter_at(ui.clip_rect());
            //     p.debug_rect(buffer_rect, Color32::RED, "buffer_rect");
            //     // p.debug_rect(buffer_clip, Color32::BLUE, "buffer_clip");
            //     p.debug_rect(history_rect, Color32::RED, "history_rect");
            //     // p.debug_rect(history_clip, Color32::BLUE, "history_clip");
            //     // p.debug_rect(viewport, Color32::GOLD, "viewport");
            // }

            if interactive {
                if let Some(pointer_pos) = ui.input().pointer.interact_pos() {
                    if buffer_response.hovered() {
                        if buffer_response.hovered() && buffer.is_mutable() {
                            ui.output().mutable_text_under_cursor = true;
                        }

                        // TODO: triple-click to select whole paragraph
                        // TODO: drag selected text to either move or clone (ctrl on windows, alt on mac)
                        let singleline_offset = vec2(state.singleline_offset, 0.0);
                        let cursor_at_pointer = buffer_galley.cursor_from_pos(
                            pointer_pos - buffer_response.rect.min + singleline_offset,
                        );

                        if ui.visuals().text_cursor_preview
                            && buffer_response.hovered()
                            && ui.input().pointer.is_moving()
                        {
                            // preview:
                            paint_cursor_end(
                                ui,
                                row_height,
                                &buffer_painter,
                                buffer_response.rect.min,
                                &buffer_galley,
                                &cursor_at_pointer,
                            );
                        }

                        if buffer_response.double_clicked() {
                            // Select word:
                            let center = cursor_at_pointer;
                            let ccursor_range = select_word_at(buffer.as_ref(), center.ccursor);
                            state.set_cursor_range(Some(Section::Buffer(CursorRange {
                                primary: buffer_galley.from_ccursor(ccursor_range.primary),
                                secondary: buffer_galley.from_ccursor(ccursor_range.secondary),
                            })));
                        } else if allow_drag_to_select {
                            if buffer_response.hovered() && ui.input().pointer.any_pressed() {
                                ui.memory().request_focus(buffer_id);
                                if ui.input().modifiers.shift {
                                    if let Some(Section::Buffer(mut cursor_range)) =
                                        state.cursor_range(&*history_galley, &*buffer_galley)
                                    {
                                        cursor_range.primary = cursor_at_pointer;
                                        state.set_cursor_range(Some(Section::Buffer(cursor_range)));
                                    } else {
                                        state.set_cursor_range(Some(Section::Buffer(
                                            CursorRange::one(cursor_at_pointer),
                                        )));
                                    }
                                } else {
                                    state.set_cursor_range(Some(Section::Buffer(
                                        CursorRange::one(cursor_at_pointer),
                                    )));
                                }
                            } else if ui.input().pointer.any_down()
                                && buffer_response.is_pointer_button_down_on()
                            {
                                // drag to select text:
                                if let Some(Section::Buffer(mut cursor_range)) =
                                    state.cursor_range(&*history_galley, &*buffer_galley)
                                {
                                    cursor_range.primary = cursor_at_pointer;
                                    state.set_cursor_range(Some(Section::Buffer(cursor_range)));
                                }
                            }
                        }
                    } else if history_response.hovered() {
                        // TODO: triple-click to select whole paragraph
                        // TODO: drag selected text to either move or clone (ctrl on windows, alt on mac)
                        let singleline_offset = vec2(state.singleline_offset, 0.0);
                        let cursor_at_pointer = history_galley.cursor_from_pos(
                            pointer_pos - history_response.rect.min + singleline_offset,
                        );

                        if ui.visuals().text_cursor_preview
                            && history_response.hovered()
                            && ui.input().pointer.is_moving()
                        {
                            // preview:
                            paint_cursor_end(
                                ui,
                                row_height,
                                &history_painter,
                                history_response.rect.min,
                                &history_galley,
                                &cursor_at_pointer,
                            );
                        }

                        if history_response.double_clicked() {
                            // Select word:
                            let center = cursor_at_pointer;
                            let ccursor_range = select_word_at(history.as_ref(), center.ccursor);
                            state.set_cursor_range(Some(Section::History(CursorRange {
                                primary: history_galley.from_ccursor(ccursor_range.primary),
                                secondary: history_galley.from_ccursor(ccursor_range.secondary),
                            })));
                        } else if allow_drag_to_select {
                            if history_response.hovered() && ui.input().pointer.any_pressed() {
                                ui.memory().request_focus(history_id);
                                if ui.input().modifiers.shift {
                                    if let Some(Section::History(mut cursor_range)) =
                                        state.cursor_range(&*history_galley, &*buffer_galley)
                                    {
                                        cursor_range.primary = cursor_at_pointer;
                                        state
                                            .set_cursor_range(Some(Section::History(cursor_range)));
                                    } else {
                                        state.set_cursor_range(Some(Section::History(
                                            CursorRange::one(cursor_at_pointer),
                                        )));
                                    }
                                } else {
                                    state.set_cursor_range(Some(Section::History(
                                        CursorRange::one(cursor_at_pointer),
                                    )));
                                }
                            } else if ui.input().pointer.any_down()
                                && history_response.is_pointer_button_down_on()
                            {
                                // drag to select text:
                                if let Some(Section::History(mut cursor_range)) =
                                    state.cursor_range(&*history_galley, &*buffer_galley)
                                {
                                    cursor_range.primary = cursor_at_pointer;
                                    state.set_cursor_range(Some(Section::History(cursor_range)));
                                }
                            }
                        }
                    }
                }
            }

            if (buffer_response.hovered() || history_response.hovered()) && interactive {
                ui.output().cursor_icon = CursorIcon::Text;
            }

            let mut submitted = None;

            let mut cursor_range = None;
            let prev_cursor_range = state.cursor_range(&*history_galley, &*buffer_galley);
            if ui.memory().has_focus(buffer_id) && interactive {
                ui.memory().lock_focus(buffer_id, lock_focus);

                let default_cursor_range = if cursor_at_end {
                    Section::Buffer(CursorRange::one(buffer_galley.end()))
                } else {
                    Section::Buffer(CursorRange::default())
                };

                let (changed, maybe_submitted, new_cursor_range) = events(
                    ui,
                    &mut state,
                    history,
                    history_index,
                    buffer,
                    &mut history_galley,
                    &mut buffer_galley,
                    layouter,
                    buffer_id,
                    wrap_width,
                    default_cursor_range,
                );

                if changed {
                    if maybe_submitted.is_some() {
                        history_response.mark_changed();
                    }

                    buffer_response.mark_changed();
                }
                cursor_range = Some(new_cursor_range);
                submitted = maybe_submitted;
            }

            if ui.visible() {
                // Paint history text.
                let history_text_draw_pos = history_response.rect.min;
                history_painter.galley(history_text_draw_pos, history_galley.clone());

                // Paint buffer text.
                let buffer_text_draw_pos = buffer_response.rect.min;
                buffer_painter.galley(buffer_text_draw_pos, buffer_galley.clone());

                if buffer.as_ref().is_empty() && !hint_text.is_empty() {
                    let hint_text_color = ui.visuals().weak_text_color();
                    let galley =
                        hint_text.into_galley(ui, Some(true), desired_buffer_size.x, text_style);
                    galley.paint_with_fallback_color(
                        &buffer_painter,
                        buffer_response.rect.min,
                        hint_text_color,
                    );
                }

                let memory = ui.memory();
                let buffer_or_history_has_focus =
                    memory.has_focus(buffer_id) || memory.has_focus(history_id);
                drop(memory);
                if buffer_or_history_has_focus {
                    let maybe_cursor_range = state.cursor_range(&*history_galley, &*buffer_galley);
                    if let Some(Section::Buffer(cursor_range)) = maybe_cursor_range {
                        // We paint the cursor on top of the text, in case
                        // the text galley has backgrounds (as e.g. `code` snippets in markup do).
                        paint_cursor_selection(
                            ui,
                            &buffer_painter,
                            buffer_text_draw_pos,
                            &buffer_galley,
                            &cursor_range,
                        );
                        paint_cursor_end(
                            ui,
                            row_height,
                            &buffer_painter,
                            buffer_text_draw_pos,
                            &buffer_galley,
                            &cursor_range.primary,
                        );

                        if interactive && buffer.is_mutable() {
                            // egui_web uses `text_cursor_pos` when showing IME,
                            // so only set it when text is editable and visible!
                            ui.ctx().output().text_cursor_pos = Some(
                                buffer_galley
                                    .pos_from_cursor(&cursor_range.primary)
                                    .translate(buffer_response.rect.min.to_vec2())
                                    .left_top(),
                            );
                        }
                    } else if let Some(Section::History(cursor_range)) = maybe_cursor_range {
                        paint_cursor_selection(
                            ui,
                            &history_painter,
                            history_text_draw_pos,
                            &history_galley,
                            &cursor_range,
                        );
                        paint_cursor_end(
                            ui,
                            row_height,
                            &history_painter,
                            history_text_draw_pos,
                            &history_galley,
                            &cursor_range.primary,
                        );
                    }
                }
            }

            state.clone().store(ui.ctx(), id);

            let selection_changed = if let (Some(cursor_range), Some(prev_cursor_range)) =
                (cursor_range, prev_cursor_range)
            {
                prev_cursor_range.as_ccursor_range() != cursor_range.as_ccursor_range()
            } else {
                false
            };

            if buffer_response.changed() {
                buffer_response
                    .widget_info(|| WidgetInfo::text_edit(prev_text.as_str(), buffer.as_str()));
            } else if selection_changed {
                match cursor_range.unwrap() {
                    Section::History(history_range) => {
                        let char_range = history_range.primary.ccursor.index
                            ..=history_range.secondary.ccursor.index;
                        let info = WidgetInfo::text_selection_changed(char_range, history.as_str());
                        history_response
                            .ctx
                            .output()
                            .events
                            .push(OutputEvent::TextSelectionChanged(info));
                    }
                    Section::Buffer(buffer_range) => {
                        let char_range = buffer_range.primary.ccursor.index
                            ..=buffer_range.secondary.ccursor.index;
                        let info = WidgetInfo::text_selection_changed(char_range, buffer.as_str());
                        buffer_response
                            .ctx
                            .output()
                            .events
                            .push(OutputEvent::TextSelectionChanged(info));
                    }
                }
            } else {
                buffer_response
                    .widget_info(|| WidgetInfo::text_edit(prev_text.as_str(), buffer.as_str()));
            }

            ConsoleOutput {
                history_response,
                buffer_response,
                history_galley,
                buffer_galley,
                state,
                cursor_range,
                submitted,
            }
        };

        ui.allocate_ui(total_desired_size, |ui| {
            egui::ScrollArea::vertical()
                .stick_to_bottom()
                .show(ui, |ui| {
                    ui.with_layout(Layout::top_down(Align::Min), add_contents)
                })
        })
        .inner
    }
}

// ----------------------------------------------------------------------------

/// Check for (keyboard) events to edit the cursor and/or text.
#[allow(clippy::too_many_arguments)]
fn events(
    ui: &mut egui::Ui,
    state: &mut ConsoleState,
    history: &mut dyn TextBuffer,
    history_index: &mut usize,
    buffer: &mut dyn TextBuffer,
    history_galley: &mut Arc<Galley>,
    buffer_galley: &mut Arc<Galley>,
    layouter: &mut dyn FnMut(&Ui, &str, f32) -> Arc<Galley>,
    focus_id: Id,
    wrap_width: f32,
    default_cursor_range: Section<CursorRange>,
) -> (bool, Option<String>, Section<CursorRange>) {
    let mut cursor_range = state
        .cursor_range(&*history_galley, &*buffer_galley)
        .unwrap_or(default_cursor_range);

    // We feed state to the undoer both before and after handling input
    // so that the undoer creates automatic saves even when there are no events for a while.
    state.undoer.lock().feed_state(
        ui.input().time,
        &(cursor_range.as_ccursor_range(), buffer.as_ref().to_owned()),
    );

    let mut any_change = false;
    let mut submitted = None;

    for event in &ui.input().events {
        let did_mutate_text = match event {
            Event::Copy => {
                if cursor_range.is_empty() {
                    ui.ctx().output().copied_text = history.as_ref().to_owned() + buffer.as_ref();
                } else if let Section::History(cursor_range) = cursor_range {
                    ui.ctx().output().copied_text = selected_str(history, &cursor_range).to_owned();
                } else if let Section::Buffer(cursor_range) = cursor_range {
                    ui.ctx().output().copied_text = selected_str(buffer, &cursor_range).to_owned();
                }
                None
            }
            Event::Cut => {
                if cursor_range.is_empty() {
                    ui.ctx().output().copied_text = buffer.take();
                    Some(CCursorRange::default())
                } else if let Section::Buffer(cursor_range) = cursor_range {
                    ui.ctx().output().copied_text = selected_str(buffer, &cursor_range).to_owned();
                    Some(CCursorRange::one(delete_selected(buffer, &cursor_range)))
                } else {
                    // History cannot be cut, only copied.
                    None
                }
            }
            Event::Text(text_to_insert) => {
                match cursor_range {
                    // Newlines are handled by `Key::Enter`.
                    Section::Buffer(cursor_range)
                        if !text_to_insert.is_empty()
                            && text_to_insert != "\n"
                            && text_to_insert != "\r" =>
                    {
                        let mut ccursor = delete_selected(buffer, &cursor_range);
                        insert_text(&mut ccursor, buffer, text_to_insert);
                        Some(CCursorRange::one(ccursor))
                    }
                    // Can't insert into history.
                    _ => None,
                }
            }
            Event::Key {
                key: Key::Tab,
                pressed: true,
                modifiers,
            } if ui.memory().has_lock_focus(focus_id) => {
                if let Section::Buffer(cursor_range) = cursor_range {
                    let mut ccursor = delete_selected(buffer, &cursor_range);
                    if modifiers.shift {
                        // TODO: support removing indentation over a selection?
                        decrease_indentation(&mut ccursor, buffer);
                    } else {
                        insert_text(&mut ccursor, buffer, "\t");
                    }
                    Some(CCursorRange::one(ccursor))
                } else {
                    None
                }
            }

            Event::Key {
                key: Key::Enter,
                pressed: true,
                modifiers,
            } => {
                if modifiers.shift {
                    // Submit command!
                    //
                    // Wipe the undoer clean - can't undo submitting a command.
                    state.undoer = Default::default();
                    // *Don't* clear the buffer, just copy its contents.
                    submitted = Some(buffer.as_str().to_owned());

                    None
                } else if let Section::Buffer(cursor_range) = cursor_range {
                    let mut ccursor = delete_selected(buffer, &cursor_range);
                    insert_text(&mut ccursor, buffer, "\n");
                    // TODO: if code editor, auto-indent by same leading tabs, + one if the lines end on an opening bracket
                    Some(CCursorRange::one(ccursor))
                } else {
                    // Can't insert into the history.
                    None
                }
            }
            Event::Key {
                key: Key::Z,
                pressed: true,
                modifiers,
            } if modifiers.command && !modifiers.shift => {
                // TODO: redo
                if let Some((undo_ccursor_range, undo_txt)) = state
                    .undoer
                    .lock()
                    .undo(&(cursor_range.as_ccursor_range(), buffer.as_ref().to_owned()))
                {
                    buffer.replace(undo_txt);

                    if let Section::Buffer(undo_ccursor_range) = undo_ccursor_range {
                        Some(*undo_ccursor_range)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }

            Event::Key {
                key: Key::ArrowDown,
                pressed: true,
                modifiers,
            } if modifiers.alt && *history_index > 1 => {
                *history_index -= 1;
                None
            }

            Event::Key {
                key: Key::ArrowUp,
                pressed: true,
                modifiers,
            } if modifiers.alt => {
                *history_index += 1;
                None
            }

            Event::Key {
                key,
                pressed: true,
                modifiers,
            } => {
                if let Section::Buffer(cursor_range) = &mut cursor_range {
                    on_key_press(cursor_range, buffer, buffer_galley, *key, modifiers)
                } else {
                    None
                }
            }

            Event::CompositionStart => {
                state.has_ime = true;
                None
            }

            Event::CompositionUpdate(text_mark) => match cursor_range {
                Section::Buffer(cursor_range)
                    if !text_mark.is_empty()
                        && text_mark != "\n"
                        && text_mark != "\r"
                        && state.has_ime =>
                {
                    let mut ccursor = delete_selected(buffer, &cursor_range);
                    let start_cursor = ccursor;
                    insert_text(&mut ccursor, buffer, text_mark);
                    Some(CCursorRange::two(start_cursor, ccursor))
                }
                _ => None,
            },

            Event::CompositionEnd(prediction) => match cursor_range {
                Section::Buffer(cursor_range)
                    if !prediction.is_empty()
                        && prediction != "\n"
                        && prediction != "\r"
                        && state.has_ime =>
                {
                    state.has_ime = false;
                    let mut ccursor = delete_selected(buffer, &cursor_range);
                    insert_text(&mut ccursor, buffer, prediction);
                    Some(CCursorRange::one(ccursor))
                }
                _ => None,
            },

            _ => None,
        };

        if let Some(new_ccursor_range) = did_mutate_text {
            any_change = true;

            // Layout again to avoid frame delay, and to keep `text` and `galley` in sync.
            *buffer_galley = layouter(ui, buffer.as_ref(), wrap_width);

            // Set cursor_range using new galley:
            cursor_range = Section::Buffer(CursorRange {
                primary: buffer_galley.from_ccursor(new_ccursor_range.primary),
                secondary: buffer_galley.from_ccursor(new_ccursor_range.secondary),
            });
        }
    }

    state.set_cursor_range(Some(cursor_range));

    state.undoer.lock().feed_state(
        ui.input().time,
        &(cursor_range.as_ccursor_range(), buffer.as_ref().to_owned()),
    );

    (any_change, submitted, cursor_range)
}

// ----------------------------------------------------------------------------

fn paint_cursor_selection(
    ui: &mut Ui,
    painter: &Painter,
    pos: Pos2,
    galley: &Galley,
    cursor_range: &CursorRange,
) {
    if cursor_range.is_empty() {
        return;
    }

    // We paint the cursor selection on top of the text, so make it transparent:
    let color = ui.visuals().selection.bg_fill.linear_multiply(0.5);
    let [min, max] = cursor_range.sorted();
    let min = min.rcursor;
    let max = max.rcursor;

    for ri in min.row..=max.row {
        let row = &galley.rows[ri];
        let left = if ri == min.row {
            row.x_offset(min.column)
        } else {
            row.rect.left()
        };
        let right = if ri == max.row {
            row.x_offset(max.column)
        } else {
            let newline_size = if row.ends_with_newline {
                row.height() / 2.0 // visualize that we select the newline
            } else {
                0.0
            };
            row.rect.right() + newline_size
        };
        let rect = Rect::from_min_max(
            pos + vec2(left, row.min_y()),
            pos + vec2(right, row.max_y()),
        );
        painter.rect_filled(rect, 0.0, color);
    }
}

fn paint_cursor_end(
    ui: &mut Ui,
    row_height: f32,
    painter: &Painter,
    pos: Pos2,
    galley: &Galley,
    cursor: &Cursor,
) {
    let stroke = ui.visuals().selection.stroke;

    let mut cursor_pos = galley.pos_from_cursor(cursor).translate(pos.to_vec2());
    cursor_pos.max.y = cursor_pos.max.y.at_least(cursor_pos.min.y + row_height); // Handle completely empty galleys
    cursor_pos = cursor_pos.expand(1.5); // slightly above/below row

    let top = cursor_pos.center_top();
    let bottom = cursor_pos.center_bottom();

    painter.line_segment(
        [top, bottom],
        (ui.visuals().text_cursor_width, stroke.color),
    );

    if false {
        // Roof/floor:
        let extrusion = 3.0;
        let width = 1.0;
        painter.line_segment(
            [top - vec2(extrusion, 0.0), top + vec2(extrusion, 0.0)],
            (width, stroke.color),
        );
        painter.line_segment(
            [bottom - vec2(extrusion, 0.0), bottom + vec2(extrusion, 0.0)],
            (width, stroke.color),
        );
    }
}

// ----------------------------------------------------------------------------

fn selected_str<'s>(text: &'s dyn TextBuffer, cursor_range: &CursorRange) -> &'s str {
    let [min, max] = cursor_range.sorted();
    text.char_range(min.ccursor.index..max.ccursor.index)
}

fn insert_text(ccursor: &mut CCursor, text: &mut dyn TextBuffer, text_to_insert: &str) {
    ccursor.index += text.insert_text(text_to_insert, ccursor.index);
}

// ----------------------------------------------------------------------------

fn delete_selected(text: &mut dyn TextBuffer, cursor_range: &CursorRange) -> CCursor {
    let [min, max] = cursor_range.sorted();
    delete_selected_ccursor_range(text, [min.ccursor, max.ccursor])
}

fn delete_selected_ccursor_range(text: &mut dyn TextBuffer, [min, max]: [CCursor; 2]) -> CCursor {
    text.delete_char_range(min.index..max.index);
    CCursor {
        index: min.index,
        prefer_next_row: true,
    }
}

fn delete_previous_char(text: &mut dyn TextBuffer, ccursor: CCursor) -> CCursor {
    if ccursor.index > 0 {
        let max_ccursor = ccursor;
        let min_ccursor = max_ccursor - 1;
        delete_selected_ccursor_range(text, [min_ccursor, max_ccursor])
    } else {
        ccursor
    }
}

fn delete_next_char(text: &mut dyn TextBuffer, ccursor: CCursor) -> CCursor {
    delete_selected_ccursor_range(text, [ccursor, ccursor + 1])
}

fn delete_previous_word(text: &mut dyn TextBuffer, max_ccursor: CCursor) -> CCursor {
    let min_ccursor = ccursor_previous_word(text.as_ref(), max_ccursor);
    delete_selected_ccursor_range(text, [min_ccursor, max_ccursor])
}

fn delete_next_word(text: &mut dyn TextBuffer, min_ccursor: CCursor) -> CCursor {
    let max_ccursor = ccursor_next_word(text.as_ref(), min_ccursor);
    delete_selected_ccursor_range(text, [min_ccursor, max_ccursor])
}

fn delete_paragraph_before_cursor(
    text: &mut dyn TextBuffer,
    galley: &Galley,
    cursor_range: &CursorRange,
) -> CCursor {
    let [min, max] = cursor_range.sorted();
    let min = galley.from_pcursor(PCursor {
        paragraph: min.pcursor.paragraph,
        offset: 0,
        prefer_next_row: true,
    });
    if min.ccursor == max.ccursor {
        delete_previous_char(text, min.ccursor)
    } else {
        delete_selected(text, &CursorRange::two(min, max))
    }
}

fn delete_paragraph_after_cursor(
    text: &mut dyn TextBuffer,
    galley: &Galley,
    cursor_range: &CursorRange,
) -> CCursor {
    let [min, max] = cursor_range.sorted();
    let max = galley.from_pcursor(PCursor {
        paragraph: max.pcursor.paragraph,
        offset: usize::MAX, // end of paragraph
        prefer_next_row: false,
    });
    if min.ccursor == max.ccursor {
        delete_next_char(text, min.ccursor)
    } else {
        delete_selected(text, &CursorRange::two(min, max))
    }
}

// ----------------------------------------------------------------------------

/// Returns `Some(new_cursor)` if we did mutate `text`.
fn on_key_press(
    cursor_range: &mut CursorRange,
    text: &mut dyn TextBuffer,
    galley: &Galley,
    key: Key,
    modifiers: &Modifiers,
) -> Option<CCursorRange> {
    match key {
        Key::Backspace => {
            let ccursor = if modifiers.mac_cmd {
                delete_paragraph_before_cursor(text, galley, cursor_range)
            } else if let Some(cursor) = cursor_range.single() {
                if modifiers.alt || modifiers.ctrl {
                    // alt on mac, ctrl on windows
                    delete_previous_word(text, cursor.ccursor)
                } else {
                    delete_previous_char(text, cursor.ccursor)
                }
            } else {
                delete_selected(text, cursor_range)
            };
            Some(CCursorRange::one(ccursor))
        }
        Key::Delete if !(cfg!(target_os = "windows") && modifiers.shift) => {
            let ccursor = if modifiers.mac_cmd {
                delete_paragraph_after_cursor(text, galley, cursor_range)
            } else if let Some(cursor) = cursor_range.single() {
                if modifiers.alt || modifiers.ctrl {
                    // alt on mac, ctrl on windows
                    delete_next_word(text, cursor.ccursor)
                } else {
                    delete_next_char(text, cursor.ccursor)
                }
            } else {
                delete_selected(text, cursor_range)
            };
            let ccursor = CCursor {
                prefer_next_row: true,
                ..ccursor
            };
            Some(CCursorRange::one(ccursor))
        }

        Key::A if modifiers.command => {
            // select all
            *cursor_range = CursorRange::two(Cursor::default(), galley.end());
            None
        }

        Key::K if modifiers.ctrl => {
            let ccursor = delete_paragraph_after_cursor(text, galley, cursor_range);
            Some(CCursorRange::one(ccursor))
        }

        Key::U if modifiers.ctrl => {
            let ccursor = delete_paragraph_before_cursor(text, galley, cursor_range);
            Some(CCursorRange::one(ccursor))
        }

        Key::W if modifiers.ctrl => {
            let ccursor = if let Some(cursor) = cursor_range.single() {
                delete_previous_word(text, cursor.ccursor)
            } else {
                delete_selected(text, cursor_range)
            };
            Some(CCursorRange::one(ccursor))
        }

        Key::ArrowLeft | Key::ArrowRight if modifiers.is_none() && !cursor_range.is_empty() => {
            if key == Key::ArrowLeft {
                *cursor_range = CursorRange::one(cursor_range.sorted()[0]);
            } else {
                *cursor_range = CursorRange::one(cursor_range.sorted()[1]);
            }
            None
        }

        Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown | Key::Home | Key::End => {
            move_single_cursor(&mut cursor_range.primary, galley, key, modifiers);
            if !modifiers.shift {
                cursor_range.secondary = cursor_range.primary;
            }
            None
        }

        _ => None,
    }
}

fn move_single_cursor(cursor: &mut Cursor, galley: &Galley, key: Key, modifiers: &Modifiers) {
    match key {
        Key::ArrowLeft => {
            if modifiers.alt || modifiers.ctrl {
                // alt on mac, ctrl on windows
                *cursor = galley.from_ccursor(ccursor_previous_word(galley.text(), cursor.ccursor));
            } else if modifiers.mac_cmd {
                *cursor = galley.cursor_begin_of_row(cursor);
            } else {
                *cursor = galley.cursor_left_one_character(cursor);
            }
        }
        Key::ArrowRight => {
            if modifiers.alt || modifiers.ctrl {
                // alt on mac, ctrl on windows
                *cursor = galley.from_ccursor(ccursor_next_word(galley.text(), cursor.ccursor));
            } else if modifiers.mac_cmd {
                *cursor = galley.cursor_end_of_row(cursor);
            } else {
                *cursor = galley.cursor_right_one_character(cursor);
            }
        }
        Key::ArrowUp => {
            if modifiers.command {
                // mac and windows behavior
                *cursor = Cursor::default();
            } else if !modifiers.ctrl {
                // we want to hijack ctrl to use it to go back/forth in history.
                *cursor = galley.cursor_up_one_row(cursor);
            }
        }
        Key::ArrowDown => {
            if modifiers.command {
                // mac and windows behavior
                *cursor = galley.end();
            } else {
                *cursor = galley.cursor_down_one_row(cursor);
            }
        }

        Key::Home => {
            if modifiers.ctrl {
                // windows behavior
                *cursor = Cursor::default();
            } else {
                *cursor = galley.cursor_begin_of_row(cursor);
            }
        }
        Key::End => {
            if modifiers.ctrl {
                // windows behavior
                *cursor = galley.end();
            } else {
                *cursor = galley.cursor_end_of_row(cursor);
            }
        }

        _ => unreachable!(),
    }
}

// ----------------------------------------------------------------------------

fn select_word_at(text: &str, ccursor: CCursor) -> CCursorRange {
    if ccursor.index == 0 {
        CCursorRange::two(ccursor, ccursor_next_word(text, ccursor))
    } else {
        let it = text.chars();
        let mut it = it.skip(ccursor.index - 1);
        if let Some(char_before_cursor) = it.next() {
            if let Some(char_after_cursor) = it.next() {
                if is_word_char(char_before_cursor) && is_word_char(char_after_cursor) {
                    let min = ccursor_previous_word(text, ccursor + 1);
                    let max = ccursor_next_word(text, min);
                    CCursorRange::two(min, max)
                } else if is_word_char(char_before_cursor) {
                    let min = ccursor_previous_word(text, ccursor);
                    let max = ccursor_next_word(text, min);
                    CCursorRange::two(min, max)
                } else if is_word_char(char_after_cursor) {
                    let max = ccursor_next_word(text, ccursor);
                    CCursorRange::two(ccursor, max)
                } else {
                    let min = ccursor_previous_word(text, ccursor);
                    let max = ccursor_next_word(text, ccursor);
                    CCursorRange::two(min, max)
                }
            } else {
                let min = ccursor_previous_word(text, ccursor);
                CCursorRange::two(min, ccursor)
            }
        } else {
            let max = ccursor_next_word(text, ccursor);
            CCursorRange::two(ccursor, max)
        }
    }
}

fn ccursor_next_word(text: &str, ccursor: CCursor) -> CCursor {
    CCursor {
        index: next_word_boundary_char_index(text.chars(), ccursor.index),
        prefer_next_row: false,
    }
}

fn ccursor_previous_word(text: &str, ccursor: CCursor) -> CCursor {
    let num_chars = text.chars().count();
    CCursor {
        index: num_chars
            - next_word_boundary_char_index(text.chars().rev(), num_chars - ccursor.index),
        prefer_next_row: true,
    }
}

fn next_word_boundary_char_index(it: impl Iterator<Item = char>, mut index: usize) -> usize {
    let mut it = it.skip(index);
    if let Some(_first) = it.next() {
        index += 1;

        if let Some(second) = it.next() {
            index += 1;
            for next in it {
                if is_word_char(next) != is_word_char(second) {
                    break;
                }
                index += 1;
            }
        }
    }
    index
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Accepts and returns character offset (NOT byte offset!).
fn find_line_start(text: &str, current_index: CCursor) -> CCursor {
    // We know that new lines, '\n', are a single byte char, but we have to
    // work with char offsets because before the new line there may be any
    // number of multi byte chars.
    // We need to know the char index to be able to correctly set the cursor
    // later.
    let chars_count = text.chars().count();

    let position = text
        .chars()
        .rev()
        .skip(chars_count - current_index.index)
        .position(|x| x == '\n');

    match position {
        Some(pos) => CCursor::new(current_index.index - pos),
        None => CCursor::new(0),
    }
}

fn decrease_indentation(ccursor: &mut CCursor, text: &mut dyn TextBuffer) {
    let line_start = find_line_start(text.as_ref(), *ccursor);

    let remove_len = if text.as_ref()[line_start.index..].starts_with('\t') {
        Some(1)
    } else if text.as_ref()[line_start.index..]
        .chars()
        .take(text::TAB_SIZE)
        .all(|c| c == ' ')
    {
        Some(text::TAB_SIZE)
    } else {
        None
    };

    if let Some(len) = remove_len {
        text.delete_char_range(line_start.index..(line_start.index + len));
        if *ccursor != line_start {
            *ccursor -= len;
        }
    }
}
