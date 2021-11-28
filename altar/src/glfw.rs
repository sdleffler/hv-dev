use std::ops::ControlFlow;

use crate::{
    event_loop::{EventLoop, MainLoopContext},
    window::WindowKind,
};
use glfw::{Context, SwapInterval, WindowMode};
use hv::{
    input::{
        GenericAxis, GenericButton, GenericWindowEvent, InputEvent, Key, ScrollAxis,
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

pub fn run(
    title: &str,
    window_kind: WindowKind,
    resources: &mut Resources,
    event_loop: &mut impl EventLoop<GL33Context, Event = GenericWindowEvent>,
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

    let mut events_buf: Vec<GenericWindowEvent> = Vec::new();

    event_loop.init(resources, &mut context)?;

    'main: loop {
        context.window.glfw.poll_events();
        for (_, event) in glfw::flush_messages(&events_rx) {
            let generic_event = match event {
                glfw::WindowEvent::Pos(x, y) => {
                    Event::WindowPos(Point2::new(x.try_into().unwrap(), y.try_into().unwrap()))
                }
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

        let flow = event_loop.tick(resources, &mut context, &mut events_buf)?;
        events_buf.clear();

        context.window.swap_buffers();

        if let ControlFlow::Break(_) = flow {
            break 'main;
        }
    }

    Ok(())
}
