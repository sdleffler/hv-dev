use std::{ops::ControlFlow, path::PathBuf};

use hv::{
    input::{GenericAxis, GenericButton, InputEvent, Mappable},
    prelude::*,
    timer::TimeContext,
};
use luminance::context::GraphicsContext;
use resources::Resources;

use crate::scene::SceneStack;

/// Possible events external to the framework.
#[derive(Debug, Clone)]
pub enum Event<Axes, Buttons>
where
    Axes: Mappable,
    Buttons: Mappable,
{
    /// An input event mapped to the parameterized axis/button types.
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
    /// Notification that the window has been requested to be closed.
    WindowClose,
    /// Notification that the window has had one or more files dropped into it.
    WindowFileDrop(Vec<PathBuf>),
}

pub type GenericEvent = Event<GenericAxis, GenericButton>;

pub trait MainLoopContext: GraphicsContext {
    fn set_vsync(&mut self, vsync_on: bool) -> Result<()>;
}

/// A generic event loop trait.
pub trait EventLoop<C> {
    type Event;

    /// Initialize the event loop given the acquired context type.
    fn init(&mut self, _resources: &mut Resources, _context: &mut C) -> Result<()> {
        Ok(())
    }

    /// The vector provided is expected to be drained by this function. If it is not drained, the
    /// events will be cleared!
    fn tick(
        &mut self,
        resources: &mut Resources,
        context: &mut C,
        events: &mut Vec<Self::Event>,
    ) -> Result<ControlFlow<(), ()>>;
}

pub struct EventQueue<E> {
    events: Vec<E>,
}

impl<E> EventQueue<E> {
    fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub fn drain(&mut self) -> impl Iterator<Item = E> + '_ {
        self.events.drain(..)
    }
}

/// An event loop which passes events through an instance of
pub struct TimedSceneStackLoop<C, E> {
    target_fps: u32,
    scene_stack: SceneStack<C>,
    timer: TimeContext,
    event_queue: Option<EventQueue<E>>,
}

impl<C, E> TimedSceneStackLoop<C, E> {
    pub fn new(target_fps: u32, scene_stack: SceneStack<C>) -> Self {
        Self {
            target_fps,
            scene_stack,
            timer: TimeContext::new(),
            event_queue: Some(EventQueue::new()),
        }
    }

    pub fn scene_stack(&self) -> &SceneStack<C> {
        &self.scene_stack
    }

    pub fn scene_stack_mut(&mut self) -> &mut SceneStack<C> {
        &mut self.scene_stack
    }
}

impl<C: 'static, E: Send + Sync + 'static> EventLoop<C> for TimedSceneStackLoop<C, E> {
    type Event = E;

    fn tick(
        &mut self,
        resources: &mut Resources,
        context: &mut C,
        events: &mut Vec<Self::Event>,
    ) -> Result<ControlFlow<(), ()>> {
        let mut event_queue = self.event_queue.take().unwrap();
        event_queue.events.append(events);
        let prev_eq = resources.insert(event_queue);
        let dt = (self.target_fps as f32).recip();

        let res = (|| {
            self.timer.tick();
            while self.timer.check_update_time(self.target_fps) {
                self.scene_stack.update(resources, context, dt)?;
            }

            self.scene_stack.draw(
                resources,
                context,
                hv::timer::duration_to_f32(self.timer.remaining_update_time()),
            )?;

            Ok::<_, Error>(())
        })();

        self.event_queue = Some(resources.remove().unwrap_or_else(EventQueue::new));

        if let Some(prev) = prev_eq {
            resources.insert(prev);
        }

        res?;

        if self.scene_stack.is_empty() {
            Ok(ControlFlow::BREAK)
        } else {
            Ok(ControlFlow::CONTINUE)
        }
    }
}
