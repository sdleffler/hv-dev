use hv::prelude::*;
use resources::Resources;

pub struct Action<T, C> {
    #[allow(clippy::type_complexity)]
    inner: Option<Box<dyn FnOnce(&mut T, &mut Resources, &mut C) -> Result<Action<T, C>>>>,
}

impl<T, C> From<()> for Action<T, C> {
    fn from(_: ()) -> Self {
        Action::none()
    }
}

impl<T, C> Action<T, C> {
    pub fn none() -> Self {
        Self { inner: None }
    }

    pub fn new<U>(f: impl FnOnce(&mut T, &mut Resources, &mut C) -> Result<U> + 'static) -> Self
    where
        U: Into<Action<T, C>>,
    {
        Self {
            inner: Some(Box::new(|s, r, c| f(s, r, c).map(Into::into))),
        }
    }

    pub fn run(mut self, target: &mut T, res: &mut Resources, context: &mut C) -> Result<()> {
        while let Some(f) = self.inner {
            self = f(target, res, context)?;
        }

        Ok(())
    }
}

pub type UpdateAction<'s, C> = Action<SceneStackUpdate<'s, C>, C>;

impl<'s, C: 'static> UpdateAction<'s, C> {
    pub fn push(scene: impl Scene<C>) -> Self {
        Self::new(|stack, _, _| {
            stack.push(scene);
            Ok(Self::none())
        })
    }

    pub fn pop() -> Self {
        Self::new(|stack, _, _| {
            stack.pop();
            Ok(Self::none())
        })
    }

    pub fn update_next(dt: f32) -> Self {
        Self::new(move |stack, res, ctx| stack.update_next(res, ctx, dt))
    }
}

pub type DrawAction<'s, C> = Action<SceneStackDraw<'s, C>, C>;

pub trait Scene<C>: 'static {
    fn update<'s>(
        &mut self,
        res: &mut Resources,
        ctx: &mut C,
        dt: f32,
    ) -> Result<UpdateAction<'s, C>>;

    fn draw<'s>(
        &mut self,
        res: &mut Resources,
        ctx: &mut C,
        remaining_dt: f32,
    ) -> Result<DrawAction<'s, C>>;
}

pub struct SceneStackUpdate<'s, C> {
    index: usize,
    scene_stack: &'s mut SceneStack<C>,
}

impl<'s, C: 'static> SceneStackUpdate<'s, C> {
    fn new(scene_stack: &'s mut SceneStack<C>) -> Self {
        Self {
            index: scene_stack.scenes.len() - 1,
            scene_stack,
        }
    }

    pub fn scene_stack(&self) -> &SceneStack<C> {
        self.scene_stack
    }

    pub fn scene_stack_mut(&mut self) -> &mut SceneStack<C> {
        self.scene_stack
    }

    pub fn push(&mut self, scene: impl Scene<C>) {
        self.scene_stack.scenes.push(Box::new(scene));
    }

    pub fn pop(&mut self) -> Option<Box<dyn Scene<C>>> {
        let removed = self.scene_stack.scenes.pop()?;
        if self.index >= self.scene_stack.scenes.len() {
            self.index = self.scene_stack.scenes.len() - 1;
        }
        Some(removed)
    }

    pub fn update_next(
        &mut self,
        res: &mut Resources,
        context: &mut C,
        dt: f32,
    ) -> Result<UpdateAction<'s, C>> {
        if self.index > 0 {
            self.index -= 1;
            self.scene_stack.scenes[self.index].update(res, context, dt)
        } else {
            Ok(UpdateAction::none())
        }
    }
}

pub struct SceneStackDraw<'s, C> {
    index: usize,
    scene_stack: &'s mut SceneStack<C>,
}

impl<'s, C: 'static> SceneStackDraw<'s, C> {
    fn new(scene_stack: &'s mut SceneStack<C>) -> Self {
        Self {
            index: scene_stack.scenes.len() - 1,
            scene_stack,
        }
    }

    pub fn draw_next(
        &mut self,
        res: &mut Resources,
        context: &mut C,
        remaining_dt: f32,
    ) -> Result<DrawAction<'s, C>> {
        if self.index > 0 {
            self.index -= 1;
            self.scene_stack.scenes[self.index].draw(res, context, remaining_dt)
        } else {
            Ok(DrawAction::none())
        }
    }
}

impl<C> Default for SceneStack<C> {
    fn default() -> Self {
        Self { scenes: Vec::new() }
    }
}

pub struct SceneStack<C> {
    scenes: Vec<Box<dyn Scene<C>>>,
}

impl<C: 'static> SceneStack<C> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(scene: Box<dyn Scene<C>>) -> Self {
        Self {
            scenes: vec![scene],
        }
    }

    pub fn push(&mut self, scene: Box<dyn Scene<C>>) {
        self.scenes.push(scene);
    }

    pub fn is_empty(&self) -> bool {
        self.scenes.is_empty()
    }

    pub fn update(&mut self, res: &mut Resources, context: &mut C, dt: f32) -> Result<()> {
        if self.scenes.is_empty() {
            return Ok(());
        }

        let action = self.scenes.last_mut().unwrap().update(res, context, dt)?;
        action.run(&mut SceneStackUpdate::new(self), res, context)
    }

    pub fn draw(&mut self, res: &mut Resources, context: &mut C, remaining_dt: f32) -> Result<()> {
        if self.scenes.is_empty() {
            return Ok(());
        }

        let action = self
            .scenes
            .last_mut()
            .unwrap()
            .draw(res, context, remaining_dt)?;
        action.run(&mut SceneStackDraw::new(self), res, context)
    }
}

impl<C: 'static> Scene<C> for SceneStack<C> {
    fn update<'s>(
        &mut self,
        res: &mut Resources,
        context: &mut C,
        dt: f32,
    ) -> Result<UpdateAction<'s, C>> {
        self.update(res, context, dt)?;

        if self.is_empty() {
            Ok(UpdateAction::pop())
        } else {
            Ok(UpdateAction::none())
        }
    }

    fn draw<'s>(
        &mut self,
        res: &mut Resources,
        context: &mut C,
        remaining_dt: f32,
    ) -> Result<DrawAction<'s, C>> {
        self.draw(res, context, remaining_dt)?;

        Ok(DrawAction::none())
    }
}
