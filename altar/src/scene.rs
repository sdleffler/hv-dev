use hv::{prelude::*, resources::Resources, script::ScriptContext};

#[derive(Debug)]
pub struct SceneScript {
    table: LuaRegistryKey,
}

impl<'lua> FromLua<'lua> for SceneScript {
    fn from_lua(lua_value: LuaValue<'lua>, lua: &'lua Lua) -> LuaResult<Self> {
        Self::from_table(lua, LuaTable::from_lua(lua_value, lua)?).to_lua_err()
    }
}

impl SceneScript {
    pub fn from_table(lua: &Lua, table: LuaTable) -> Result<Self> {
        Ok(Self {
            table: lua.create_registry_value(table)?,
        })
    }

    /// Enter the script context, mutably loan the `World` to the `Resources` (if it's configured to
    /// accept such a loan), load the Lua table representing the script, and then run a closure w/
    /// access to the Lua context, resources, and script table.
    pub fn in_context<'lua, T>(
        &self,
        lua: &'lua Lua,
        resources: &Resources,
        script_context: &mut ScriptContext,
        thunk: impl FnOnce(&'lua Lua, &Resources, LuaTable<'lua>) -> Result<T>,
    ) -> Result<T> {
        script_context.with_resources(lua, resources, |_| {
            let table: LuaTable = lua.registry_value(&self.table)?;
            thunk(lua, resources, table)
        })?
    }

    #[allow(clippy::too_many_arguments)]
    pub fn call_method<'lua, S, A, R>(
        &self,
        lua: &'lua Lua,
        resources: &Resources,
        script_context: &mut ScriptContext,
        name: &S,
        args: A,
    ) -> Result<R>
    where
        A: ToLuaMulti<'lua>,
        R: FromLuaMulti<'lua>,
        S: AsRef<str> + ?Sized,
    {
        self.in_context(lua, resources, script_context, |_, _, script| {
            script
                .call_method(name.as_ref(), (script.clone(), args))
                .with_context(|| {
                    anyhow!(
                        "error while evaluating scene script method: {}",
                        name.as_ref()
                    )
                })
        })
    }
}

pub enum PostTick<C, T> {
    Push(Box<dyn Scene<C, T>>),
    Switch(Box<dyn Scene<C, T>>),
    Pop,
    None,
}

impl<'lua, C: 'static, T: 'static> FromLuaMulti<'lua> for PostTick<C, T> {
    fn from_lua_multi(lua_multi: LuaMultiValue<'lua>, lua: &'lua Lua) -> LuaResult<Self> {
        let (maybe_variant, maybe_ud) =
            <(Option<LuaValue>, Option<LuaAnyUserData>)>::from_lua_multi(lua_multi, lua)?;

        if let Some(v) = maybe_variant {
            let lua_str = LuaString::from_lua(v, lua)?;
            match lua_str.to_str()? {
                "Push" => {
                    let ud = maybe_ud
                        .ok_or_else(|| anyhow!("expected a scene to push!"))
                        .to_lua_err()?;
                    Ok(Self::Push(ud.dyn_take()?))
                }
                "Switch" => {
                    let ud = maybe_ud
                        .ok_or_else(|| anyhow!("expected a scene to switch!"))
                        .to_lua_err()?;
                    Ok(Self::Switch(ud.dyn_take()?))
                }
                "Pop" => Ok(Self::Pop),
                "None" => Ok(Self::None),
                _ => Err(LuaError::external("expected Push, Switch, Pop, or None!")),
            }
        } else {
            Ok(Self::None)
        }
    }
}

pub trait Scene<C, T> {
    /// Called once on the scene when it is created, before any other hooks.
    fn load(&mut self, resources: &Resources, lua: &Lua, context: &mut C) -> Result<()>;

    /// Called before `update`/`draw` on a tick.
    fn pre_tick(&mut self, resources: &Resources, lua: &Lua, context: &mut C) -> Result<()>;

    /// A logical update w/ a fixed timestep. Called zero or more times per frame, depending on how
    /// much time has passed since the last logical timestep/update.
    ///
    /// The delta-time is provided in a [`Dt`](crate::types::Dt) entry in the resource map.
    fn update(&mut self, resources: &Resources, lua: &Lua, context: &mut C) -> Result<()>;

    /// If true, also update the scene "below" this on the scene stack. Default implementation
    /// returns `false`.
    fn update_previous(&self) -> bool {
        false
    }

    /// A render update. Always called once per tick.
    ///
    /// The remaining delta-time (difference between the time of the last logical update and the
    /// time of this render) is given in a [`RemainingDt`](crate::types::RemainingDt) entry in the
    /// resource map.
    fn draw(&mut self, resources: &Resources, lua: &Lua, context: &mut C, target: &T)
        -> Result<()>;

    /// If true, also draw the scene "below" this on the scene stack. Useful if you're writing a
    /// `Scene` for dialogue and you still want to draw the game scene, etc. Default implementation
    /// returns `false`.
    fn draw_previous(&self) -> bool {
        false
    }

    /// Called after all `update` and `draw` calls for a tick are finished. Returns an action to
    /// take for the entire tick, which allows modifying the scene stack.
    fn post_tick(
        &mut self,
        resources: &Resources,
        lua: &Lua,
        context: &mut C,
    ) -> Result<PostTick<C, T>>;
}

pub struct SceneStack<C, T> {
    scenes: Vec<Box<dyn Scene<C, T>>>,
}

impl<C, T> SceneStack<C, T> {
    pub fn empty() -> Self {
        Self { scenes: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.scenes.is_empty()
    }

    pub fn push(&mut self, scene: Box<dyn Scene<C, T>>) {
        self.scenes.push(scene);
    }

    pub fn pre_tick(&mut self, resources: &Resources, lua: &Lua, context: &mut C) -> Result<()> {
        let current = self
            .scenes
            .last_mut()
            .ok_or_else(|| anyhow!("empty scene stack!"))?;
        current.pre_tick(resources, lua, context)
    }

    pub fn update(&mut self, resources: &Resources, lua: &Lua, context: &mut C) -> Result<bool> {
        for scene in self.scenes.iter_mut().rev() {
            scene.update(resources, lua, context)?;
            if !scene.update_previous() {
                return Ok(false);
            }
        }

        Ok(true)
    }

    pub fn post_tick(&mut self, resources: &Resources, lua: &Lua, context: &mut C) -> Result<()> {
        let current = self
            .scenes
            .last_mut()
            .ok_or_else(|| anyhow!("empty scene stack!"))?;
        match current.post_tick(resources, lua, context)? {
            PostTick::Push(scene) => self.scenes.push(scene),
            PostTick::Switch(scene) => *current = scene,
            PostTick::Pop => drop(self.scenes.pop()),
            PostTick::None => {}
        }

        Ok(())
    }

    pub fn draw(
        &mut self,
        resources: &Resources,
        lua: &Lua,
        context: &mut C,
        target: &T,
    ) -> Result<bool> {
        for scene in self.scenes.iter_mut().rev() {
            scene.draw(resources, lua, context, target)?;
            if !scene.draw_previous() {
                return Ok(false);
            }
        }

        Ok(true)
    }
}
