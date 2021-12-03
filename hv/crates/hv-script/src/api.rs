use alloc::{borrow::ToOwned, boxed::Box, string::String};
use anyhow::{bail, Result};
use hv_alchemy::Type;
use hv_lua::prelude::*;

pub struct Module {
    name: String,
    #[allow(clippy::type_complexity)]
    build: Box<
        dyn for<'lua> Fn(&'lua Lua, &mut ModuleBuilder<'lua>) -> Result<()> + Send + Sync + 'static,
    >,
}

impl Module {
    pub fn new<S, F>(name: &S, closure: F) -> Self
    where
        F: for<'lua> Fn(&'lua Lua, &mut ModuleBuilder<'lua>) -> Result<()> + Send + Sync + 'static,
        S: AsRef<str> + ?Sized,
    {
        Self {
            name: name.as_ref().to_owned(),
            build: Box::new(closure),
        }
    }

    pub fn build<'lua>(&self, lua: &'lua Lua) -> Result<ModuleBuilder<'lua>> {
        let mut builder = ModuleBuilder::new(lua)?;
        (self.build)(lua, &mut builder)?;
        Ok(builder)
    }
}

#[derive(Debug, Clone)]
pub struct ModuleBuilder<'lua> {
    lua: &'lua Lua,
    table: LuaTable<'lua>,
}

impl<'lua> ToLua<'lua> for ModuleBuilder<'lua> {
    fn to_lua(self, _lua: &'lua Lua) -> LuaResult<LuaValue<'lua>> {
        Ok(LuaValue::Table(self.table))
    }
}

impl<'lua> ModuleBuilder<'lua> {
    pub fn new(lua: &'lua Lua) -> Result<Self> {
        let table = lua.create_table()?;
        Ok(Self { lua, table })
    }

    pub fn value<S>(&mut self, name: &S, value: impl ToLua<'lua>) -> Result<&mut Self>
    where
        S: AsRef<str> + ?Sized,
    {
        let name = name.as_ref();
        if self.table.contains_key(name)? {
            bail!("module already contains an entry named `{}`!", name);
        }
        self.table.set(name, value.to_lua(self.lua)?)?;
        Ok(self)
    }

    pub fn function<S, F, A, R>(&mut self, name: &S, closure: F) -> Result<&mut Self>
    where
        F: for<'lua2> Fn(&'lua2 Lua, A) -> Result<R> + Send + 'static,
        A: FromLuaMulti<'lua>,
        R: ToLuaMulti<'lua>,
        S: AsRef<str> + ?Sized,
    {
        self.value(
            name,
            self.lua.create_function(move |lua, args| {
                closure(lua, args).map_err(|err| {
                    err.downcast::<LuaError>()
                        .unwrap_or_else(|other| other.to_lua_err())
                })
            })?,
        )
    }

    pub fn function_mut<S, F, A, R>(&mut self, name: &S, mut closure: F) -> Result<&mut Self>
    where
        F: for<'lua2> FnMut(&'lua2 Lua, A) -> Result<R> + Send + 'static,
        A: FromLuaMulti<'lua>,
        R: ToLuaMulti<'lua>,
        S: AsRef<str>,
    {
        self.value(
            name,
            self.lua.create_function_mut(move |lua, args| {
                closure(lua, args).map_err(|err| {
                    err.downcast::<LuaError>()
                        .unwrap_or_else(|other| other.to_lua_err())
                })
            })?,
        )
    }

    pub fn userdata_type<T>(&mut self, name: &str) -> Result<&mut Self>
    where
        T: LuaUserData + Send + 'static,
    {
        self.value(name, Type::<T>::of())
    }

    pub fn submodule(&mut self, sub: &Module) -> Result<&mut Self> {
        let name = sub.name.clone();
        self.value(&name, sub.build(self.lua)?)
    }

    pub fn build(&self) -> LuaTable<'lua> {
        self.table.clone()
    }
}

lazy_static::lazy_static! {
    pub static ref HV: Module = Module::new("hv", hv);
    pub static ref ECS: Module = Module::new("ecs", ecs);
    pub static ref MATH: Module = Module::new("math", math);
}

fn hv<'lua>(_lua: &'lua Lua, builder: &mut ModuleBuilder<'lua>) -> Result<()> {
    builder.submodule(&*ECS)?.submodule(&*MATH)?;

    Ok(())
}

fn ecs<'lua>(_lua: &'lua Lua, builder: &mut ModuleBuilder<'lua>) -> Result<()> {
    use hv_ecs::*;

    builder
        .userdata_type::<World>("World")?
        .userdata_type::<DynamicQuery>("Query")?;

    Ok(())
}

fn math<'lua>(_lua: &'lua Lua, builder: &mut ModuleBuilder<'lua>) -> Result<()> {
    use hv_math::*;

    builder
        .userdata_type::<Vector2<f32>>("Vector2")?
        .userdata_type::<Vector3<f32>>("Vector3")?
        .userdata_type::<Isometry2<f32>>("Isometry2")?
        .userdata_type::<Isometry3<f32>>("Isometry3")?
        .userdata_type::<Velocity2<f32>>("Velocity2")?;

    Ok(())
}
