pub extern crate egui;

use egui::{DroppedFile, Event, Pos2, RawInput, Rect, Vec2};
use hv_input::{
    GenericAxis, GenericButton, GenericInputState, GenericWindowEvent, InputEvent, KeyMods,
    ScrollAxis, WindowEvent,
};

pub struct GuiInputState {
    time: f32,
    event_queue: Vec<GenericWindowEvent>,
    hv_input_state: GenericInputState,
    raw_input_state: RawInput,
    content_scale: f32,
}

impl Default for GuiInputState {
    fn default() -> Self {
        Self::new()
    }
}

impl GuiInputState {
    pub fn new() -> Self {
        Self {
            time: 0.,
            event_queue: Vec::new(),
            hv_input_state: GenericInputState::new(),
            raw_input_state: RawInput::default(),
            content_scale: 1.,
        }
    }

    pub fn push_event(&mut self, event: GenericWindowEvent) {
        self.event_queue.push(event);
    }

    pub fn flush(&mut self, dt: f32) -> RawInput {
        use WindowEvent::*;
        self.time += dt;
        self.raw_input_state.time = Some(self.time.into());
        self.raw_input_state.predicted_dt = dt;
        for event in self.event_queue.drain(..) {
            match event {
                Mapped(input_event) => {
                    self.hv_input_state.update_event(input_event);

                    match input_event {
                        InputEvent::Axis(axis, state) => match axis {
                            GenericAxis::Mouse(ScrollAxis::Horizontal) => self
                                .raw_input_state
                                .events
                                .push(Event::Scroll(Vec2::new(state * self.content_scale, 0.))),
                            GenericAxis::Mouse(ScrollAxis::Vertical) => self
                                .raw_input_state
                                .events
                                .push(Event::Scroll(Vec2::new(0., state * self.content_scale))),
                            _ => {}
                        },
                        InputEvent::Button { button, state, .. } => match button {
                            GenericButton::KeyCode(hv_input::Key::C, mods)
                                if mods.ctrl && !(mods.shift || mods.alt || mods.cmd) =>
                            {
                                self.raw_input_state.events.push(Event::Copy)
                            }
                            GenericButton::KeyCode(hv_input::Key::X, mods)
                                if mods.ctrl && !(mods.shift || mods.alt || mods.cmd) =>
                            {
                                self.raw_input_state.events.push(Event::Cut)
                            }
                            GenericButton::KeyCode(key, mods) => {
                                if let Some(ek) = to_egui_key(key) {
                                    self.raw_input_state.events.push(Event::Key {
                                        key: ek,
                                        pressed: state,
                                        modifiers: to_egui_modifiers(mods),
                                    });
                                }
                            }
                            GenericButton::Mouse(button, mods) => {
                                if let Some(ep) = to_egui_mb(button) {
                                    let mouse_pos = self.hv_input_state.mouse_position();
                                    let pos = Pos2::new(
                                        mouse_pos.x / self.content_scale,
                                        mouse_pos.y / self.content_scale,
                                    );
                                    self.raw_input_state.events.push(Event::PointerButton {
                                        pos,
                                        button: ep,
                                        pressed: state,
                                        modifiers: to_egui_modifiers(mods),
                                    });
                                }
                            }
                            _ => {}
                        },
                        InputEvent::Cursor(pos) => {
                            let pos =
                                Pos2::new(pos.x / self.content_scale, pos.y / self.content_scale);
                            self.raw_input_state.events.push(Event::PointerMoved(pos));
                        }
                    }
                }
                Text(c) => {
                    // Egui does not want to receive enter key text events. We send those from
                    // mapped enter key presses. So we skip JUST enter key events.
                    if c != '\n' || c != '\t' {
                        self.raw_input_state.events.push(Event::Text(c.to_string()))
                    }
                }
                ContentScale(v) => {
                    // TODO: *properly* warn if v.x != v.y.
                    assert_eq!(v.x, v.y, "weird content scale");
                    self.raw_input_state.pixels_per_point = Some(v.x);
                    self.content_scale = v.x;
                }
                WindowCursorEnter(entered) => {
                    if !entered {
                        self.raw_input_state.events.push(Event::PointerGone);
                    }
                }
                WindowFocus(focused) => {
                    if !focused {
                        self.raw_input_state.events.push(Event::PointerGone);
                    }
                }
                WindowFileDrop(paths) => {
                    self.raw_input_state
                        .dropped_files
                        .extend(paths.into_iter().map(|path| DroppedFile {
                            name: path.display().to_string(),
                            path: Some(path),
                            last_modified: None,
                            bytes: None,
                        }));
                }
                FramebufferSize(_) => {}
                WindowSize(size) => {
                    self.raw_input_state.screen_rect = Some(Rect::from_min_size(
                        Pos2::ZERO,
                        Vec2::new(
                            size.x as f32 / self.content_scale,
                            size.y as f32 / self.content_scale,
                        ),
                    ));
                }
                WindowPos(_) | WindowMinimize(_) | WindowMaximize(_) | WindowRefresh
                | WindowClose => {}
            }
        }

        self.raw_input_state.take()
    }
}

fn to_egui_key(key: hv_input::Key) -> Option<egui::Key> {
    use egui::Key as Ek;
    use hv_input::Key as Hk;
    let ek = match key {
        Hk::Down => Ek::ArrowDown,
        Hk::Left => Ek::ArrowLeft,
        Hk::Right => Ek::ArrowRight,
        Hk::Up => Ek::ArrowUp,

        Hk::Escape => Ek::Escape,
        Hk::Tab => Ek::Tab,
        Hk::Backspace => Ek::Backspace,
        Hk::Enter => Ek::Enter,
        Hk::Space => Ek::Space,

        Hk::Insert => Ek::Insert,
        Hk::Delete => Ek::Delete,
        Hk::Home => Ek::Home,
        Hk::End => Ek::End,
        Hk::PageUp => Ek::PageUp,
        Hk::PageDown => Ek::PageDown,

        Hk::Num0 => Ek::Num0,
        Hk::Num1 => Ek::Num1,
        Hk::Num2 => Ek::Num2,
        Hk::Num3 => Ek::Num3,
        Hk::Num4 => Ek::Num4,
        Hk::Num5 => Ek::Num5,
        Hk::Num6 => Ek::Num6,
        Hk::Num7 => Ek::Num7,
        Hk::Num8 => Ek::Num8,
        Hk::Num9 => Ek::Num9,

        Hk::A => Ek::A, // Used for cmd+A (select All)
        Hk::B => Ek::B,
        Hk::C => Ek::C,
        Hk::D => Ek::D,
        Hk::E => Ek::E,
        Hk::F => Ek::F,
        Hk::G => Ek::G,
        Hk::H => Ek::H,
        Hk::I => Ek::I,
        Hk::J => Ek::J,
        Hk::K => Ek::K, // Used for ctrl+K (delete text after cursor)
        Hk::L => Ek::L,
        Hk::M => Ek::M,
        Hk::N => Ek::N,
        Hk::O => Ek::O,
        Hk::P => Ek::P,
        Hk::Q => Ek::Q,
        Hk::R => Ek::R,
        Hk::S => Ek::S,
        Hk::T => Ek::T,
        Hk::U => Ek::U, // Used for ctrl+U (delete text before cursor)
        Hk::V => Ek::V,
        Hk::W => Ek::W, // Used for ctrl+W (delete previous word)
        Hk::X => Ek::X,
        Hk::Y => Ek::Y,
        Hk::Z => Ek::Z, // Used for cmd+Z (undo)

        _ => return None,
    };

    Some(ek)
}

fn to_egui_mb(button: hv_input::MouseButton) -> Option<egui::PointerButton> {
    use egui::PointerButton as Ep;
    use hv_input::MouseButton as Hm;
    match button {
        Hm::Button1 => Some(Ep::Primary),
        Hm::Button2 => Some(Ep::Secondary),
        Hm::Button3 => Some(Ep::Middle),
        _ => None,
    }
}

fn to_egui_modifiers(mods: KeyMods) -> egui::Modifiers {
    egui::Modifiers {
        alt: mods.alt,
        ctrl: mods.ctrl,
        shift: mods.shift,
        mac_cmd: false,
        command: mods.cmd,
    }
}
