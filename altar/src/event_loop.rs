use std::{ops::ControlFlow, slice::Iter, vec::Drain};

use hv::{prelude::*, resources::Resources};
use luminance::context::GraphicsContext;

/// A resource which holds events from a given tick.
///
/// This is intended to be filled and then either read or drained by a consumer, and cleared before
/// every fill; it is not intended to accumulate events across ticks. For that, an
/// [`EventChannel`](shrev::EventChannel) is more appropriate.
#[derive(Debug)]
pub struct TickEvents<E> {
    queue: Vec<E>,
}

impl<E> Default for TickEvents<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E> Extend<E> for TickEvents<E> {
    fn extend<T: IntoIterator<Item = E>>(&mut self, iter: T) {
        self.queue.extend(iter);
    }
}

impl<E> TickEvents<E> {
    pub fn new() -> Self {
        Self { queue: Vec::new() }
    }

    pub fn clear(&mut self) {
        self.queue.clear();
    }

    pub fn push(&mut self, event: E) {
        self.queue.push(event);
    }

    pub fn drain(&mut self) -> Drain<E> {
        self.queue.drain(..)
    }

    pub fn iter(&self) -> Iter<E> {
        self.queue.iter()
    }
}

pub trait MainLoopContext: GraphicsContext {
    fn set_vsync(&mut self, vsync_on: bool) -> Result<()>;
}

/// A generic event loop trait.
pub trait EventLoop<C> {
    /// Initialize the event loop given the acquired context type.
    fn init(&mut self, _resources: &mut Resources, _lua: &Lua, _context: &mut C) -> Result<()> {
        Ok(())
    }

    /// The vector provided is expected to be drained by this function. If it is not drained, the
    /// events will be cleared!
    fn tick(
        &mut self,
        resources: &mut Resources,
        lua: &Lua,
        context: &mut C,
    ) -> Result<ControlFlow<(), ()>>;
}

pub trait FixedTimestepLoop<C> {
    fn init(&mut self, _resources: &mut Resources, _lua: &Lua, _context: &mut C) -> Result<()> {
        Ok(())
    }

    fn pre_tick(&mut self, resources: &mut Resources, lua: &Lua, context: &mut C) -> Result<()>;

    fn update(&mut self, resources: &mut Resources, lua: &Lua, context: &mut C) -> Result<()>;
    fn draw(&mut self, resources: &mut Resources, lua: &Lua, context: &mut C) -> Result<()>;

    fn post_tick(
        &mut self,
        resources: &mut Resources,
        lua: &Lua,
        context: &mut C,
    ) -> Result<ControlFlow<(), ()>>;
}
