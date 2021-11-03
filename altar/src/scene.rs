use hv::prelude::*;
use resources::Resources;

pub struct Action<T> {
    #[allow(clippy::type_complexity)]
    inner: Option<Box<dyn FnOnce(&mut T, &mut Resources) -> Result<Action<T>>>>,
}

impl<T> From<()> for Action<T> {
    fn from(_: ()) -> Self {
        Action::none()
    }
}

impl<T> Action<T> {
    pub fn none() -> Self {
        Self { inner: None }
    }

    pub fn new<U>(f: impl FnOnce(&mut T, &mut Resources) -> Result<U> + 'static) -> Self
    where
        U: Into<Action<T>>,
    {
        Self {
            inner: Some(Box::new(|s, r| f(s, r).map(Into::into))),
        }
    }

    pub fn run(mut self, ctx: &mut T, res: &mut Resources) -> Result<()> {
        while let Some(f) = self.inner {
            self = f(ctx, res)?;
        }

        Ok(())
    }
}

pub type UpdateAction<'s> = Action<SceneStackUpdate<'s>>;

impl<'s> UpdateAction<'s> {
    pub fn push(scene: impl Scene) -> Self {
        Self::new(|ctx, _| {
            ctx.push(scene);
            Ok(Self::none())
        })
    }

    pub fn pop() -> Self {
        Self::new(|ctx, _| {
            ctx.pop();
            Ok(Self::none())
        })
    }

    pub fn update_next(dt: f32) -> Self {
        Self::new(move |ctx, res| ctx.update_next(res, dt))
    }
}

pub type DrawAction<'s> = Action<SceneStackDraw<'s>>;

pub trait Scene: 'static {
    fn update<'s>(&mut self, res: &mut Resources, dt: f32) -> Result<UpdateAction<'s>>;
    fn draw<'s>(&mut self, res: &mut Resources) -> Result<DrawAction<'s>>;
}

pub struct SceneStackUpdate<'s> {
    index: usize,
    scene_stack: &'s mut SceneStack,
}

impl<'s> SceneStackUpdate<'s> {
    fn new(scene_stack: &'s mut SceneStack) -> Self {
        Self {
            index: scene_stack.scenes.len() - 1,
            scene_stack,
        }
    }

    pub fn scene_stack(&self) -> &SceneStack {
        self.scene_stack
    }

    pub fn scene_stack_mut(&mut self) -> &mut SceneStack {
        self.scene_stack
    }

    pub fn push(&mut self, scene: impl Scene) {
        self.scene_stack.scenes.push(Box::new(scene));
    }

    pub fn pop(&mut self) -> Option<Box<dyn Scene>> {
        let removed = self.scene_stack.scenes.pop()?;
        if self.index >= self.scene_stack.scenes.len() {
            self.index = self.scene_stack.scenes.len() - 1;
        }
        Some(removed)
    }

    pub fn update_next(&mut self, res: &mut Resources, dt: f32) -> Result<UpdateAction<'s>> {
        if self.index > 0 {
            self.index -= 1;
            self.scene_stack.scenes[self.index].update(res, dt)
        } else {
            Ok(UpdateAction::none())
        }
    }
}

pub struct SceneStackDraw<'s> {
    index: usize,
    scene_stack: &'s mut SceneStack,
}

impl<'s> SceneStackDraw<'s> {
    fn new(scene_stack: &'s mut SceneStack) -> Self {
        Self {
            index: scene_stack.scenes.len() - 1,
            scene_stack,
        }
    }

    pub fn draw_next(&mut self, res: &mut Resources) -> Result<DrawAction<'s>> {
        if self.index > 0 {
            self.index -= 1;
            self.scene_stack.scenes[self.index].draw(res)
        } else {
            Ok(DrawAction::none())
        }
    }
}

#[derive(Default)]
pub struct SceneStack {
    scenes: Vec<Box<dyn Scene>>,
}

impl SceneStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.scenes.is_empty()
    }

    pub fn update(&mut self, res: &mut Resources, dt: f32) -> Result<()> {
        if self.scenes.is_empty() {
            return Ok(());
        }

        let action = self.scenes.last_mut().unwrap().update(res, dt)?;
        action.run(&mut SceneStackUpdate::new(self), res)
    }

    pub fn draw(&mut self, res: &mut Resources) -> Result<()> {
        if self.scenes.is_empty() {
            return Ok(());
        }

        let action = self.scenes.last_mut().unwrap().draw(res)?;
        action.run(&mut SceneStackDraw::new(self), res)
    }
}

impl Scene for SceneStack {
    fn update<'s>(&mut self, res: &mut Resources, dt: f32) -> Result<UpdateAction<'s>> {
        self.update(res, dt)?;

        if self.is_empty() {
            Ok(UpdateAction::pop())
        } else {
            Ok(UpdateAction::none())
        }
    }

    fn draw<'s>(&mut self, res: &mut Resources) -> Result<DrawAction<'s>> {
        self.draw(res)?;

        Ok(DrawAction::none())
    }
}
