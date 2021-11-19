//! Components, systems, and resources, for loading, running/updating, and managing resources
//! provided to Lua scripts.

use std::{any::TypeId, collections::HashMap, io::Read};

use hv::{
    alchemy::{AlchemicalAny, AlchemicalAnyExt},
    ecs::{QueryMarker, SystemContext, Without},
    fs::Filesystem,
    lua::TryCloneToUserDataExt,
    prelude::*,
    sync::{
        cell::ArcCell,
        elastic::{Elastic, ScopeArena, ScopeGuard, StretchedMut, StretchedRef},
    },
};
use tracing::{error, trace_span};

use crate::{command_buffer::CommandPoolResource, types::Dt};

/// A type-indexed map which maps [`TypeId`]s to [`Elastic<StretchedMut<T>>`] and
/// [`Elastic<StretchedRef<T>>`] for [`T: UserData`](LuaUserData), and provides Lua access to these
/// stretched/loaned values.
///
/// Values can be registered with this type using either [`ScriptResources::insert_ref`] or
/// [`ScriptResources::insert_mut`]. Using either one is a *commitment* to how you plan to access
/// the type in the future, and to how you expect scripts/Lua to access the type. In the future it
/// may be possible to lift this requirement and dynamically check whether a type has been loaned by
/// reference or by mutable reference, but not now.
#[derive(Clone)]
pub struct ScriptResources {
    map: ArcCell<HashMap<TypeId, Box<dyn AlchemicalAny + Send + Sync>>>,
}

impl Default for ScriptResources {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptResources {
    /// Create an empty [`ScriptResources`] object, capable of loaning data to Lua.
    pub fn new() -> Self {
        Self {
            map: ArcCell::default(),
        }
    }

    /// Insert a resource which can be accessed immutably. Attempts to mutably access it on the Lua
    /// side will cause an error. Loans to this type must be done using [`loan_ref`].
    pub fn insert_ref<T: LuaUserData + Send + Sync + 'static>(
        &self,
        elastic: Elastic<StretchedRef<T>>,
    ) {
        self.map
            .as_inner()
            .borrow_mut()
            .insert(TypeId::of::<T>(), Box::new(elastic))
            .expect("already a resource of this type in the map!");
    }

    /// Insert a resource which can be accessed mutably. Loans to the entry for this type must be
    /// done using [`loan_mut`].
    pub fn insert_mut<T: LuaUserData + Send + Sync + 'static>(
        &self,
        elastic: Elastic<StretchedMut<T>>,
    ) {
        self.map
            .as_inner()
            .borrow_mut()
            .insert(TypeId::of::<T>(), Box::new(elastic))
            .expect("already a resource of this type in the map!");
    }

    /// Immutably loan a resource, receiving back a scope guard and a reference to the "stretched"
    /// type.
    ///
    /// # Safety
    ///
    /// The returned `ElasticGuard` *must* be dropped by the end of its lifetime.
    pub fn loan_ref<'a, T: LuaUserData + 'static>(
        &self,
        guard: &mut ScopeGuard<'a>,
        val: &'a T,
    ) -> Option<Elastic<StretchedRef<T>>> {
        let elastic = self
            .map
            .as_inner()
            .borrow()
            .get(&TypeId::of::<T>())?
            .downcast_ref::<Elastic<StretchedRef<T>>>()?
            .clone();
        guard.loan(&elastic, val);
        Some(elastic)
    }

    /// Mutably loan a resource, receiving back a scope guard and a reference to the "stretched"
    /// type.
    ///
    /// # Safety
    ///
    /// The returned `ElasticGuard` *must* be dropped by the end of its lifetime.
    pub fn loan_mut<'a, T: LuaUserData + 'static>(
        &self,
        guard: &mut ScopeGuard<'a>,
        val: &'a mut T,
    ) -> Option<Elastic<StretchedMut<T>>> {
        let elastic = self
            .map
            .as_inner()
            .borrow()
            .get(&TypeId::of::<T>())?
            .downcast_ref::<Elastic<StretchedMut<T>>>()?
            .clone();
        guard.loan(&elastic, val);
        Some(elastic)
    }

    /// Get a resource and dynamically convert it to userdata without requiring a static type.
    pub fn get_userdata<'lua>(
        &self,
        lua: &'lua Lua,
        type_id: TypeId,
    ) -> Option<LuaAnyUserData<'lua>> {
        Some(
            self.map
                .as_inner()
                .borrow()
                .get(&type_id)?
                .dyncast_ref::<dyn TryCloneToUserDataExt>()
                .expect("Elastic should always succeed dyncast to dyn TryCloneToUserData!")
                .try_clone_to_user_data(lua)
                .expect("Elastic should always succeed clone to userdata! ... unless it's empty"),
        )
    }
}

impl LuaUserData for ScriptResources {
    fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
        // Given a userdata `Type<T>` object, try to extract the corresponding resource and return
        // it, converted to userdata.
        methods.add_method("get", |lua, this, ud_type: LuaAnyUserData| {
            Ok(ud_type
                .meta_type_table()
                .ok()
                .map(|type_table| this.get_userdata(lua, type_table.id)))
        });
    }
}

/// A component type which represents the path of a script to load and attach to a component.
#[derive(Debug)]
pub struct Script {
    pub path: String,
}

/// A component type indicating that a script failed to load, used as both a diagnostic and a marker
/// to avoid attempting to load the same buggy script multiple times.
#[derive(Debug)]
pub struct ScriptLoadError {
    pub error: Error,
}

/// A resource type, representing a context in which to load and run scripts. Holds references to a
/// [`ScriptResources`] object used to loan external resources into Lua as well as a Lua registry
/// key referring to a Lua table used as the global environment table for Lua scripts.
pub struct ScriptContext {
    // Environment table for scripts loaded in this context
    env: LuaRegistryKey,
    resources: ScriptResources,
}

/// The string name of the Lua global which holds the [`ScriptResources`].
pub const SCRIPT_RESOURCES: &str = "_RESOURCES";

static_assertions::assert_impl_all!(ScriptContext: Send, Sync);

impl ScriptContext {
    /// Create a new script context w/ a parent environment table.
    ///
    /// The environment table is used as the `__index` of the metatable of a newly created table
    /// which holds a reference to a [`ScriptResources`] object, with the string key
    /// [`SCRIPT_RESOURCES`]. [`ScriptResources`] allows Lua scripts access to external resources
    /// which are available for only a non-`'static` period; see the struct docs for more.
    pub fn new(lua: &Lua, env_table: LuaTable) -> Result<Self> {
        let env_mt = lua.create_table()?;
        env_mt.set("__index", env_table)?;
        let wrapped_env_table = lua.create_table()?;
        wrapped_env_table.set_metatable(Some(env_mt));
        let resources = ScriptResources::new();
        wrapped_env_table.set("_RESOURCES", resources.clone())?;
        let env = lua.create_registry_value(wrapped_env_table)?;
        Ok(Self { env, resources })
    }

    /// Get the [`ScriptResources`] object.
    pub fn resources(&self) -> &ScriptResources {
        &self.resources
    }

    /// Get the environment table used by this [`ScriptContext`].
    pub fn env(&self) -> &LuaRegistryKey {
        &self.env
    }
}

/// A local system which attempts to load new scripts from [`Script`] components and attach them as
/// [`LuaRegistryKey`] components. Failing to load a script will cause a [`ScriptLoadError`]
/// containing the error to be attached instead, and the error will also be logged at the `Error`
/// level.
pub fn script_upkeep_system(
    context: SystemContext,
    (scope_arena, script_context, fs, command_pool): (
        &ScopeArena,
        &mut ScriptContext,
        &mut Filesystem,
        &CommandPoolResource,
    ),
    lua: &Lua,
    (unloaded_scripts,): (QueryMarker<Without<ScriptLoadError, Without<LuaRegistryKey, &Script>>>,),
) {
    let _span = trace_span!("script_upkeep_system").entered();
    scope_arena.scope(|scope| {
        let fs = script_context
            .resources
            .loan_mut(scope, fs)
            .expect("no filesystem resource!");
        let mut command_buffer = command_pool.get_buffer();

        let mut buf = String::new();
        let env_table: LuaTable = lua.registry_value(&script_context.env).unwrap();

        for (entity, script) in context.query(unloaded_scripts).iter() {
            let res = (|| -> Result<LuaRegistryKey> {
                let mut mut_fs = fs.borrow_mut().unwrap();
                buf.clear();
                mut_fs.open(&script.path)?.read_to_string(&mut buf)?;
                drop(mut_fs);
                let loaded: LuaValue = lua
                    .load(&buf)
                    .set_name(&script.path)?
                    .set_environment(env_table.clone())?
                    .call(())?;
                let registry_key = lua.create_registry_value(loaded)?;
                Ok(registry_key)
            })();

            match res {
                Ok(key) => command_buffer.insert(entity, (key,)),
                Err(error) => {
                    error!(
                        ?entity,
                        script = ?script.path,
                        ?error,
                        "error instantiating entity script: {:#}",
                        error
                    );

                    command_buffer.insert(entity, (ScriptLoadError { error },));
                }
            }
        }
    });
}

/// A local system which looks for entities with [`Script`] and [`LuaRegistryKey`] components;
/// and tries to extract a table from the [`LuaRegistryKey`] component and run an `update` method on
/// it if it exists. If calling the `update` method fails, an error will be logged.
pub fn script_update_system(
    context: SystemContext,
    (scope_arena, script_context, fs, &Dt(dt)): (
        &ScopeArena,
        &mut ScriptContext,
        &mut Filesystem,
        &Dt,
    ),
    lua: &Lua,
    (with_registry_key,): (QueryMarker<(&Script, &LuaRegistryKey)>,),
) {
    let _span = trace_span!("script_update_system").entered();
    scope_arena.scope(|scope| {
        let _fs = script_context.resources.loan_mut(scope, fs);

        for (entity, (script, key)) in context.query(with_registry_key).iter() {
            let res = (|| -> Result<()> {
                let table = lua.registry_value::<LuaTable>(key)?;
                if table.contains_key("update")? {
                    let _: () = table.call_method("update", (dt,))?;
                }
                Ok(())
            })();

            if let Err(err) = res {
                error!(
                    entity = ?entity,
                    script = ?script.path,
                    error = ?err,
                    "error calling entity script update: {:#}",
                    err
                );
            }
        }
    });
}
