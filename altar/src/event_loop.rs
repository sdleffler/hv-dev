use std::ops::ControlFlow;

use hv::{prelude::*, resources::Resources, timer::TimeContext};
use luminance::context::GraphicsContext;

use crate::{
    scene::SceneStack,
    types::{Dt, RemainingDt},
};

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

/// An event loop which passes events through an instance of [`EventQueue`] stored in the passed
/// [`Resources`], and which performs timing calculations to separate update and render steps and
/// calls those steps on a provided [`SceneStack`].
///
/// The [`EventLoop`] implementation for this type will return [`ControlFlow::Break`] when the
/// scenestack is empty.
pub struct TimedSceneStackLoop<C, E> {
    target_fps: u32,
    scene_stack: SceneStack<C>,
    timer: TimeContext,
    event_queue: Option<EventQueue<E>>,
}

impl<C, E> TimedSceneStackLoop<C, E> {
    /// Create a new timed scene-stack based event loop with the given FPS target and a scene stack
    /// to start from.
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

        // In case there was an `EventQueue` already in the `Resources`, we hold on to it (and put
        // it back later.)
        let prev_eq = resources.insert(event_queue);
        let dt = (self.target_fps as f32).recip();
        resources.entry::<Dt>().or_insert(Dt(dt));

        let res = (|| {
            self.timer.tick();
            while self.timer.check_update_time(self.target_fps) {
                self.scene_stack.update(resources, context)?;
            }

            resources.entry::<RemainingDt>().or_insert_with(|| {
                RemainingDt(hv::timer::duration_to_f32(
                    self.timer.remaining_update_time(),
                ))
            });

            self.scene_stack.draw(resources, context)?;

            Ok::<_, Error>(())
        })();

        self.event_queue = Some(resources.remove().unwrap_or_else(EventQueue::new));

        // We put back the event queue that was there before us, if it exists.
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
