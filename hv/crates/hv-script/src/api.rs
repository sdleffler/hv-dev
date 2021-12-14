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

use std::io::Read;

use anyhow::{anyhow, bail, Context, Result};
use hv_alchemy::Type;
use hv_elastic::{ElasticMut, ElasticRef};
use hv_filesystem::Filesystem;
use hv_lua::prelude::*;
use hv_resources::Resources;

pub struct Module {
    name: String,
    key: String,
    #[allow(clippy::type_complexity)]
    build: Box<dyn for<'lua> Fn(&'lua Lua) -> Result<ModuleBuilder<'lua>> + Send + Sync + 'static>,
}

impl Module {
    pub fn new<S1, S2, F>(name: &S1, key: &S2, closure: F) -> Self
    where
        F: for<'lua> Fn(&'lua Lua) -> Result<ModuleBuilder<'lua>> + Send + Sync + 'static,
        S1: AsRef<str> + ?Sized,
        S2: AsRef<str> + ?Sized,
    {
        Self {
            name: name.as_ref().to_owned(),
            key: key.as_ref().to_owned(),
            build: Box::new(closure),
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

    pub fn to_table(&self) -> LuaTable<'lua> {
        self.table.clone()
    }
}

lazy_static::lazy_static! {
    /// Entrypoint for the main `hv` Lua API.
    ///
    /// Exposes all `hv.*` modules, and overrides a number of native Lua functions.
    ///
    /// ## Override behavior
    ///
    /// Some Lua functions are overridden/replaced rather than being entirely removed, and in such a
    /// way that they interface more nicely with Heavy. For example, the `package.loaders` table is
    /// modified in such a way that Lua cannot access the normal filesystem, and instead uses the
    /// Heavy `hv-filesystem`'s `Filesystem` type (if available) to load Lua code. However, many Lua
    /// functions are either detrimental to portability, potentially insecure (though security isn't
    /// something Heavy focuses too heavily on, being a game framework) and altogether just don't
    /// make all that much sense.
    ///
    /// ### Override behavior: modified functions and tables
    ///
    /// - `package`: `package.loaders` completely replaced w/ Heavy-specific alternatives, thus
    ///   changing the behavior of `require`.
    /// - `print`: results in logging a `DEBUG` message instead.
    /// - `require`: paths are loaded from the Heavy VFS rather than from the OS filesystem.
    ///
    /// ### Override behavior: removed functions and tables
    ///
    /// - `load`, `loadfile`, `loadstring`: often a code smell and ideally unnecessary. These could
    ///   be replaced at some point with a Heavy VFS enabled version which also maintains the
    ///   correct function environment, but we currently have no good reason to do so.
    /// - `module`: we don't need it, it's considered a bit of a code smell, and there's no reason
    ///   to keep it for completeness when so often we replace the global environment for scripts.
    /// - `io`: superseded by the Heavy filesystem; in the future a compatibility layer could be
    ///   implemented but currently we have no good reason to do so.
    /// - `os`: unneeded and a big potential problem for a misbehaving script.
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

    /// A convenient state machine/pushdown automaton abstraction.
    ///
    /// Loaded automatically by the `hv` module as `hv.lua.agent`.
    pub static ref AGENT: Module = Module::new("agent", "hv.lua.agent", hv_lua_agent);
}

fn hv_filesystem_loader<'lua>(lua: &'lua Lua, path: LuaString<'lua>) -> LuaResult<LuaValue<'lua>> {
    // `package.path`
    let package_path = lua
        .globals()
        .get::<_, LuaTable>("package")?
        .get::<_, LuaString>("path")?;

    // Shouldn't ever be bad unicode, tbh. If either of these are bad unicode, something else is
    // very wrong.
    let package_path = package_path.to_str()?;
    let path = path.to_str()?;

    // First, look for a `Filesystem` in our Lua app data.
    if let Some(mut fs) = lua.app_data_mut::<Filesystem>() {
        return hv_filesystem_do_load(lua, package_path, path, &mut *fs);
    } else if let Some(fs_elastic) = lua.app_data_ref::<ElasticMut<Filesystem>>() {
        if let Ok(mut fs) = fs_elastic.try_borrow_mut() {
            return hv_filesystem_do_load(lua, package_path, path, &mut *fs);
        }
    }

    // If there's no `Filesystem` in our Lua app data, check for a `Resources` there instead.
    if let Some(resources_elastic) = lua.app_data_ref::<ElasticRef<Resources>>() {
        if let Ok(resources) = resources_elastic.try_borrow() {
            if let Ok(mut fs) = resources.get_mut::<Filesystem>() {
                return hv_filesystem_do_load(lua, package_path, path, &mut *fs);
            } else if let Ok(fs_elastic) = resources.get::<ElasticMut<Filesystem>>() {
                if let Ok(mut fs) = fs_elastic.try_borrow_mut() {
                    return hv_filesystem_do_load(lua, package_path, path, &mut *fs);
                }
            }
        }
    }

    "could not find a `Filesystem` resource to search!".to_lua(lua)
}

fn hv_filesystem_do_load<'lua>(
    lua: &'lua Lua,
    package_path: &str,
    path: &str,
    fs: &mut Filesystem,
) -> LuaResult<LuaValue<'lua>> {
    let segments = package_path.split(';');
    let path_replaced = path.replace(".", "/");
    let mut tried = Vec::new();

    for segment in segments {
        let path = segment.replace('?', &path_replaced);
        let mut file = match fs.open(&path) {
            Ok(file) => file,
            Err(err) => {
                tried.push(err.to_string());
                continue;
            }
        };
        let mut buf = String::new();
        file.read_to_string(&mut buf).to_lua_err()?;
        let loaded = lua
            .load(&buf)
            .set_name(&path)?
            .into_function()
            .with_context(|| anyhow!("error while loading module {}", path))
            .to_lua_err()?;

        return Some(loaded).to_lua(lua);
    }

    // FIXME: better error reporting here; collect errors from individual module attempts
    // and log them?
    Some(format!("module {} not found: {}\n", path, tried.join("\n"),)).to_lua(lua)
}

fn hv(lua: &Lua) -> Result<ModuleBuilder> {
    // Clean the global namespace; see override behavior
    let g = lua.globals();
    g.raw_remove("load")?;
    g.raw_remove("loadfile")?;
    g.raw_remove("loadstring")?;
    g.raw_remove("module")?;
    g.raw_remove("io")?;
    g.raw_remove("os")?;

    let package: LuaTable = g.get("package")?;
    let loaders: LuaTable = package.get("loaders")?;

    // Set the value of `package.path` such that it is compatible with our loaders and the Heavy
    // VFS's requirement of all paths starting at the root.
    package.set("path", "/?.lua;/?/init.lua")?;

    // cpath does not apply. Clear it to emphasize this and keep note.
    package.set("cpath", "")?;

    // Remove the default filesystem search and C library opening Lua loader behavior; we do not
    // want to search the OS filesystem in any way.
    loaders.raw_remove(3)?;
    loaders.raw_remove(2)?;

    // Add in a filesystem loader which loads from the Heavy VFS rather than the OS filesystem.
    loaders.set(2, lua.create_function(hv_filesystem_loader)?)?;

    let mut builder = ModuleBuilder::new(lua)?;
    builder
        .submodule(&*HV_ECS)?
        .submodule(&*HV_MATH)?
        .submodule(&*HV_LUA)?;

    Ok(builder)
}

fn hv_ecs(lua: &Lua) -> Result<ModuleBuilder> {
    use hv_ecs::*;
    let mut builder = ModuleBuilder::new(lua)?;
    builder
        .userdata_type::<World>("World")?
        .userdata_type::<DynamicQuery>("Query")?;

    Ok(builder)
}

fn hv_math(lua: &Lua) -> Result<ModuleBuilder> {
    use hv_math::*;
    let mut builder = ModuleBuilder::new(lua)?;
    builder
        .userdata_type::<Vector2<f32>>("Vector2")?
        .userdata_type::<Vector3<f32>>("Vector3")?
        .userdata_type::<Isometry2<f32>>("Isometry2")?
        .userdata_type::<Isometry3<f32>>("Isometry3")?
        .userdata_type::<Velocity2<f32>>("Velocity2")?;

    Ok(builder)
}

fn hv_lua(lua: &Lua) -> Result<ModuleBuilder> {
    let mut builder = ModuleBuilder::new(lua)?;
    builder
        .userdata_type::<LuaRegistryKey>("RegistryKey")?
        .submodule(&*BINSER)?
        .submodule(&*CLASS)?
        .submodule(&*AGENT)?;

    Ok(builder)
}

fn hv_lua_agent(lua: &Lua) -> Result<ModuleBuilder> {
    let _ = CLASS.build(lua)?;
    Ok(ModuleBuilder::from_table(
        lua,
        lua.load(include_str!("../resources/agent.lua")).eval()?,
    ))
}
