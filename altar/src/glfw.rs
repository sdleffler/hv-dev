use std::{cell::RefCell, collections::HashMap, ops::ControlFlow, rc::Rc};

use crate::{
    event_loop::{EventLoop, MainLoopContext, TickEvents},
    window::WindowKind,
};
use glfw::{Context, Joystick, JoystickEvent, JoystickId, SwapInterval, WindowMode};
use hv::{
    input::{
        GenericAxis, GenericButton, GenericWindowEvent, InputEvent, InputState, Key, ScrollAxis,
        WindowEvent as Event,
    },
    prelude::*,
    resources::Resources,
};
use luminance_glfw::{GL33Context, GlfwSurface, GlfwSurfaceError};

impl MainLoopContext for GL33Context {
    fn set_vsync(&mut self, vsync_on: bool) -> Result<()> {
        let interval = if vsync_on {
            glfw::SwapInterval::Sync(1)
        } else {
            glfw::SwapInterval::None
        };
        self.window.glfw.set_swap_interval(interval);
        Ok(())
    }
}

fn joystick_callback(
    id: JoystickId,
    event: JoystickEvent,
    userdata: &Rc<RefCell<Vec<(JoystickId, JoystickEvent)>>>,
) {
    userdata.borrow_mut().push((id, event));
}

struct GamepadEntry {
    joystick: Joystick,
    input_state: InputState<glfw::GamepadAxis, glfw::GamepadButton>,
}

pub fn run(
    title: &str,
    window_kind: WindowKind,
    resources: &mut Resources,
    event_loop: &mut impl EventLoop<GL33Context>,
) -> Result<()> {
    let GlfwSurface {
        events_rx,
        mut context,
    } = GlfwSurface::new(|glfw| {
        let (mut window, events) = match window_kind {
            WindowKind::Fullscreen { width, height } => glfw.with_primary_monitor(|glfw, m| {
                let m = m.ok_or_else(|| {
                    GlfwSurfaceError::UserError(anyhow!(
                        "no primary monitor - cannot create fullscreen window"
                    ))
                })?;
                glfw.create_window(
                    width as u32,
                    height as u32,
                    title,
                    WindowMode::FullScreen(m),
                )
                .ok_or_else(|| {
                    GlfwSurfaceError::UserError(anyhow!("failed to create fullscreen GLFW window!"))
                })
            })?,
            WindowKind::Windowed { width, height } => glfw
                .create_window(width, height, title, WindowMode::Windowed)
                .ok_or_else(|| {
                    GlfwSurfaceError::UserError(anyhow!("failed to create GLFW window!"))
                })?,
        };

        window.make_current();
        window.set_all_polling(true);
        glfw.set_swap_interval(SwapInterval::Sync(1));

        Ok((window, events))
    })
    .map_err(|err| anyhow!("error initializing glfw window: {}", err))?;

    resources.insert({
        let mut events = TickEvents::<GenericWindowEvent>::new();

        let (window_size_x, window_size_y) = context.window.get_size();
        let (fb_size_x, fb_size_y) = context.window.get_framebuffer_size();
        let (cs_x, cs_y) = context.window.get_content_scale();

        events.extend([
            Event::WindowSize(Vector2::new(window_size_x as u32, window_size_y as u32)),
            Event::FramebufferSize(Vector2::new(fb_size_x as u32, fb_size_y as u32)),
            Event::ContentScale(Vector2::new(cs_x, cs_y)),
        ]);

        events
    });

    let joystick_events = Rc::new(RefCell::new(Vec::new()));
    let mut gamepad_map = HashMap::new();

    context
        .window
        .glfw
        .set_joystick_callback(Some(glfw::Callback {
            f: joystick_callback,
            data: joystick_events.clone(),
        }));

    let lua = crate::api::create_lua_context()?;

    event_loop.init(resources, &lua, &mut context)?;

    'main: loop {
        context.window.glfw.poll_events();
        let mut events_buf = resources.get_mut::<TickEvents<GenericWindowEvent>>()?;
        for (_, event) in glfw::flush_messages(&events_rx) {
            let generic_event = match event {
                glfw::WindowEvent::Pos(x, y) => Event::WindowPos(Point2::new(x, y)),
                glfw::WindowEvent::Size(w, h) => {
                    Event::WindowSize(Vector2::new(w.try_into().unwrap(), h.try_into().unwrap()))
                }
                glfw::WindowEvent::Close => Event::WindowClose,
                glfw::WindowEvent::Refresh => Event::WindowRefresh,
                glfw::WindowEvent::Focus(focused) => Event::WindowFocus(focused),
                glfw::WindowEvent::Iconify(iconified) => Event::WindowMinimize(iconified),
                glfw::WindowEvent::FramebufferSize(w, h) => Event::FramebufferSize(Vector2::new(
                    w.try_into().unwrap(),
                    h.try_into().unwrap(),
                )),
                glfw::WindowEvent::MouseButton(button, action, modifiers) => {
                    let keymods = modifiers.into();
                    let (state, repeat) = match action {
                        glfw::Action::Press => (true, false),
                        glfw::Action::Repeat => (true, true),
                        glfw::Action::Release => (false, false),
                    };
                    Event::Mapped(InputEvent::Button {
                        button: GenericButton::Mouse(button.into(), keymods),
                        state,
                        repeat,
                    })
                }
                glfw::WindowEvent::CursorPos(x, y) => {
                    Event::Mapped(InputEvent::Cursor(Point2::new(x as f32, y as f32)))
                }
                glfw::WindowEvent::CursorEnter(entered) => Event::WindowCursorEnter(entered),
                glfw::WindowEvent::Scroll(x, y) => {
                    events_buf.push(Event::Mapped(InputEvent::Axis(
                        GenericAxis::Mouse(ScrollAxis::Vertical),
                        y as f32,
                    )));
                    events_buf.push(Event::Mapped(InputEvent::Axis(
                        GenericAxis::Mouse(ScrollAxis::Horizontal),
                        x as f32,
                    )));
                    continue;
                }
                glfw::WindowEvent::Key(key, _scancode, action, modifiers) => {
                    let (state, repeat) = match action {
                        glfw::Action::Press => (true, false),
                        glfw::Action::Repeat => (true, true),
                        glfw::Action::Release => (false, false),
                    };
                    let keymods = modifiers.into();
                    events_buf.push(Event::Mapped(InputEvent::Button {
                        // glfw key presses are pre-translated (unlike SDL) so they match our
                        // definition of "scancode". So each one will emit two events - scancode and
                        // keycode. Some things will listen only for keycodes and only for
                        // scancodes; it's GLFW's fault that it conflates them such that it thinks
                        // they're the same thing.
                        button: GenericButton::KeyCode(Key::from(key), keymods),
                        state,
                        repeat,
                    }));
                    events_buf.push(Event::Mapped(InputEvent::Button {
                        // glfw key presses are pre-translated (unlike SDL) so they match our
                        // definition of "scancode". So each one will emit two events - scancode and
                        // keycode. Some things will listen only for keycodes and only for
                        // scancodes; it's GLFW's fault that it conflates them such that it thinks
                        // they're the same thing.
                        button: GenericButton::ScanCode(Key::from(key), keymods),
                        state,
                        repeat,
                    }));
                    continue;
                }
                glfw::WindowEvent::Char(c) => Event::Text(c),
                // Currently ignore the custom unicode input event (glfwSetCharModsCallback)
                glfw::WindowEvent::CharModifiers(..) => continue,
                glfw::WindowEvent::FileDrop(paths) => Event::WindowFileDrop(paths),
                glfw::WindowEvent::Maximize(maximized) => Event::WindowMaximize(maximized),
                glfw::WindowEvent::ContentScale(x, y) => Event::ContentScale(Vector2::new(x, y)),
            };

            events_buf.push(generic_event);
        }

        for (id, event) in joystick_events.borrow_mut().drain(..) {
            match event {
                JoystickEvent::Connected => {
                    let joystick = context.window.glfw.get_joystick(id);
                    if joystick.is_gamepad() {
                        gamepad_map.insert(
                            id,
                            GamepadEntry {
                                joystick,
                                input_state: InputState::new(),
                            },
                        );
                    }
                }
                JoystickEvent::Disconnected => {
                    gamepad_map.remove(&id);
                }
            }
        }

        for entry in gamepad_map.values_mut() {
            let state = match entry.joystick.get_gamepad_state() {
                Some(state) => state,
                None => continue,
            };
            entry.input_state.update(0.);

            for button in (0..).map_while(glfw::GamepadButton::from_i32) {
                let button_state = state.get_button_state(button);
                match button_state {
                    glfw::Action::Press => entry.input_state.update_button_down(button, false),
                    glfw::Action::Release => entry.input_state.update_button_up(button),
                    glfw::Action::Repeat => entry.input_state.update_button_down(button, true),
                }

                if entry.input_state.get_button_pressed(button) {
                    events_buf.push(Event::Mapped(InputEvent::Button {
                        button: GenericButton::Gamepad(button.into()),
                        state: true,
                        repeat: false,
                    }));
                } else if entry.input_state.get_button_up(button) {
                    events_buf.push(Event::Mapped(InputEvent::Button {
                        button: GenericButton::Gamepad(button.into()),
                        state: false,
                        repeat: false,
                    }));
                }
            }

            for axis in (0..).map_while(glfw::GamepadAxis::from_i32) {
                let axis_state = state.get_axis(axis);
                events_buf.push(Event::Mapped(InputEvent::Axis(
                    GenericAxis::Gamepad(axis.into()),
                    if axis_state.abs() > 0.03 {
                        axis_state
                    } else {
                        0.0
                    },
                )));
            }
        }

        drop(events_buf);

        if let ControlFlow::Break(_) = event_loop.tick(resources, &lua, &mut context)? {
            break 'main;
        }

        context.window.swap_buffers();

        // Clear the tick events last so that "initialization" events get through.
        resources
            .get_mut::<TickEvents<GenericWindowEvent>>()?
            .clear();
    }

    Ok(())
}
