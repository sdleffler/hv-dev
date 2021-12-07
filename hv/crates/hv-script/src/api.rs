//! Lua API loaders exposing Heavy and other Rust to Lua.
//!
//! You probably want to load the `hv` library into your Heavy project by default. It's easy:
//!
//! ```
//! #mod hv { pub use hv_script as script };
//! #use hv_lua::prelude::*;
//! #use anyhow::Result;
//! #fn thing(lua: &Lua) -> Result<()> {
//! hv::script::api::HV.load(lua)?;
//! #Ok(()) }
//! ```

use alloc::{borrow::ToOwned, boxed::Box, string::String};
use anyhow::{bail, Result};
use hv_alchemy::Type;
use hv_lua::prelude::*;

pub struct Module {
    name: String,
    key: String,
    #[allow(clippy::type_complexity)]
    build: Box<dyn for<'lua> Fn(&'lua Lua) -> Result<ModuleBuilder<'lua>> + Send + Sync + 'static>,
}

impl Module {
    pub fn new<S1, S2, F>(name: &S1, key: &S2, closure: F) -> Self
    where
        F: for<'lua> Fn(&'lua Lua, &mut ModuleBuilder<'lua>) -> Result<()> + Send + Sync + 'static,
        S1: AsRef<str> + ?Sized,
        S2: AsRef<str> + ?Sized,
    {
        Self {
            name: name.as_ref().to_owned(),
            key: key.as_ref().to_owned(),
            build: Box::new(move |lua| {
                let mut builder = ModuleBuilder::new(lua)?;
                closure(lua, &mut builder)?;
                Ok(builder)
            }),
        }
    }

    pub fn from_source<S1, S2, T>(name: &S1, key: &S2, source: &T) -> Self
    where
        S1: AsRef<str> + ?Sized,
        S2: AsRef<str> + ?Sized,
        T: AsRef<[u8]> + ?Sized,
    {
        let bytes = source.as_ref().to_owned();
        Self {
            name: name.as_ref().to_owned(),
            key: key.as_ref().to_owned(),
            build: Box::new(move |lua| {
                let table = lua.load(&bytes).eval()?;
                Ok(ModuleBuilder::from_table(lua, table))
            }),
        }
    }

    pub fn build<'lua>(&self, lua: &'lua Lua) -> Result<ModuleBuilder<'lua>> {
        let mut out = None;
        lua.scope(|scope| {
            let func = scope.create_function(|lua, ()| (self.build)(lua).to_lua_err())?;
            out = Some(lua.load_from_function(&self.key, func)?);
            Ok(())
        })?;
        Ok(out.unwrap())
    }

    pub fn load(&self, lua: &Lua) -> Result<()> {
        lua.globals().set(self.name.as_str(), self.build(lua)?)?;
        Ok(())
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

impl<'lua> FromLua<'lua> for ModuleBuilder<'lua> {
    fn from_lua(lua_value: LuaValue<'lua>, lua: &'lua Lua) -> LuaResult<Self> {
        let table = LuaTable::from_lua(lua_value, lua)?;
        Ok(Self { lua, table })
    }
}

impl<'lua> ModuleBuilder<'lua> {
    pub fn new(lua: &'lua Lua) -> Result<Self> {
        let table = lua.create_table()?;
        Ok(Self { lua, table })
    }

    pub fn from_table(lua: &'lua Lua, table: LuaTable<'lua>) -> Self {
        Self { lua, table }
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
    /// Entrypoint for the main `hv` Lua API.
    ///
    /// Exposes all `hv.*` modules.
    pub static ref HV: Module = Module::new("hv", "hv", hv);

    /// The `hv.ecs` module.
    pub static ref HV_ECS: Module = Module::new("ecs", "hv.ecs", hv_ecs);

    /// The `hv.math` module.
    pub static ref HV_MATH: Module = Module::new("math", "hv.math", hv_math);

    /// The `hv.lua` module.
    ///
    /// Also exposes `hv.lua.binser` (the `binser` serialization library, also [`BINSER`]) and
    /// `hv.lua.class` (the `30log` object orientation/prototyping library, also [`CLASS`].)
    pub static ref HV_LUA: Module = Module::new("lua", "hv.lua", hv_lua);

    /// The `binser` Lua serialization library.
    ///
    /// Loaded automatically by the `hv` module as `hv.lua.binser`.
    pub static ref BINSER: Module =
        Module::from_source("binser", "hv.lua.binser", include_str!("../resources/binser.lua"));

    /// The `30log` Lua object orientation/prototyping library.
    ///
    /// Loaded automatically by the `hv` module as `hv.lua.class`.
    pub static ref CLASS: Module =
        Module::from_source("class", "hv.lua.class", include_str!("../resources/class.lua"));
}

fn hv<'lua>(_lua: &'lua Lua, builder: &mut ModuleBuilder<'lua>) -> Result<()> {
    builder
        .submodule(&*HV_ECS)?
        .submodule(&*HV_MATH)?
        .submodule(&*HV_LUA)?;

    Ok(())
}

fn hv_ecs<'lua>(_lua: &'lua Lua, builder: &mut ModuleBuilder<'lua>) -> Result<()> {
    use hv_ecs::*;

    builder
        .userdata_type::<World>("World")?
        .userdata_type::<DynamicQuery>("Query")?;

    Ok(())
}

fn hv_math<'lua>(_lua: &'lua Lua, builder: &mut ModuleBuilder<'lua>) -> Result<()> {
    use hv_math::*;

    builder
        .userdata_type::<Vector2<f32>>("Vector2")?
        .userdata_type::<Vector3<f32>>("Vector3")?
        .userdata_type::<Isometry2<f32>>("Isometry2")?
        .userdata_type::<Isometry3<f32>>("Isometry3")?
        .userdata_type::<Velocity2<f32>>("Velocity2")?;

    Ok(())
}

fn hv_lua<'lua>(_lua: &'lua Lua, builder: &mut ModuleBuilder<'lua>) -> Result<()> {
    builder
        .userdata_type::<LuaRegistryKey>("RegistryKey")?
        .submodule(&*BINSER)?
        .submodule(&*CLASS)?;

    Ok(())
}
