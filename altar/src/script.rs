//! Components, systems, and resources, for loading, running/updating, and managing resources
//! provided to Lua scripts.

use std::{
    any::TypeId,
    collections::{HashMap, HashSet},
    fmt,
};

use hv::{
    alchemy::{AlchemicalAny, AlchemicalAnyExt},
    bump::{Bump, Owned},
    prelude::*,
    resources::{self, Resources},
    sync::{
        cell::{ArcRef, ArcRefMut},
        elastic::{Elastic, ScopeArena, ScopeGuard, StretchedMut, StretchedRef},
    },
};

struct ScriptResource {
    inner: Box<dyn AlchemicalAny + Send + Sync>,
    loanable: for<'a> fn(&'a Resources, &'a Bump) -> Result<Owned<'a, dyn ErasedLoan<'a> + 'a>>,
    name: String,
}

impl fmt::Debug for ScriptResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ScriptResource {{ name: {} }}", self.name)
    }
}

impl ScriptResource {
    fn with_mut<T: 'static + Send + Sync>(elastic: Elastic<StretchedMut<T>>) -> Self {
        fn loan_mut<'a, T: 'static>(
            resources: &'a Resources,
            alloc: &'a Bump,
        ) -> Result<Owned<'a, dyn ErasedLoan<'a> + 'a>> {
            let guard = resources.get_mut::<T>()?;
            let owned = Owned::new_in(guard, alloc);
            // We have to do a pointer cast here since `Owned` can't deal with `CoerceUnsized`.
            let raw = Owned::into_raw(owned) as *mut dyn ErasedLoan<'a>;
            Ok(unsafe { Owned::from_raw(raw) })
        }

        Self {
            inner: Box::new(elastic),
            loanable: loan_mut::<T>,
            name: std::any::type_name::<T>().to_owned(),
        }
    }

    fn with_ref<T: 'static + Send + Sync>(elastic: Elastic<StretchedRef<T>>) -> Self {
        fn loan_ref<'a, T: 'static>(
            resources: &'a Resources,
            alloc: &'a Bump,
        ) -> Result<Owned<'a, dyn ErasedLoan<'a> + 'a>> {
            let guard = resources.get::<T>()?;
            let owned = Owned::new_in(guard, alloc);
            // We have to do a pointer cast here since `Owned` can't deal with `CoerceUnsized`.
            let raw = Owned::into_raw(owned) as *mut dyn ErasedLoan<'a>;
            Ok(unsafe { Owned::from_raw(raw) })
        }

        Self {
            inner: Box::new(elastic),
            loanable: loan_ref::<T>,
            name: std::any::type_name::<T>().to_owned(),
        }
    }

    fn with_arc_ref<T: 'static + Send + Sync>(elastic: Elastic<StretchedRef<T>>) -> Self {
        fn loan_arc_ref<'a, T: 'static>(
            resources: &'a Resources,
            alloc: &'a Bump,
        ) -> Result<Owned<'a, dyn ErasedLoan<'a> + 'a>> {
            let guard = resources.get::<Elastic<StretchedRef<T>>>()?;
            let owned = Owned::new_in(
                guard
                    .borrow_arc(|t| &**t)
                    .ok_or_else(|| anyhow!("failed to mutably borrow elastic"))?,
                alloc,
            );
            let raw = Owned::into_raw(owned) as *mut dyn ErasedLoan<'a>;
            Ok(unsafe { Owned::from_raw(raw) })
        }

        Self {
            inner: Box::new(elastic),
            loanable: loan_arc_ref::<T>,
            name: std::any::type_name::<T>().to_owned(),
        }
    }

    fn with_arc_mut<T: 'static + Send + Sync>(elastic: Elastic<StretchedMut<T>>) -> Self {
        fn loan_arc_mut<'a, T: 'static>(
            resources: &'a Resources,
            alloc: &'a Bump,
        ) -> Result<Owned<'a, dyn ErasedLoan<'a> + 'a>> {
            let guard = resources.get::<Elastic<StretchedMut<T>>>()?;
            let owned = Owned::new_in(
                guard
                    .borrow_arc_mut(|t| &mut **t)
                    .ok_or_else(|| anyhow!("failed to mutably borrow elastic"))?,
                alloc,
            );
            let raw = Owned::into_raw(owned) as *mut dyn ErasedLoan<'a>;
            Ok(unsafe { Owned::from_raw(raw) })
        }

        Self {
            inner: Box::new(elastic),
            loanable: loan_arc_mut::<T>,
            name: std::any::type_name::<T>().to_owned(),
        }
    }
}

trait ErasedLoan<'a> {
    fn erased_loan<'b>(
        &'b mut self,
        guard: &mut ScopeGuard<'b>,
        map: &HashMap<TypeId, ScriptResource>,
    ) -> Result<()>
    where
        'a: 'b;
}

impl<'a, T: 'static + AlchemicalAny> ErasedLoan<'a> for resources::Ref<'a, T> {
    fn erased_loan<'b>(
        &'b mut self,
        guard: &mut ScopeGuard<'b>,
        map: &HashMap<TypeId, ScriptResource>,
    ) -> Result<()>
    where
        'a: 'b,
    {
        let elastic = (*map.get(&TypeId::of::<T>()).unwrap().inner)
            .downcast_ref::<Elastic<StretchedRef<T>>>()
            .unwrap();
        guard.loan(elastic, &**self);

        Ok(())
    }
}

impl<'a, T: 'static + AlchemicalAny> ErasedLoan<'a> for resources::RefMut<'a, T> {
    fn erased_loan<'b>(
        &'b mut self,
        guard: &mut ScopeGuard<'b>,
        map: &HashMap<TypeId, ScriptResource>,
    ) -> Result<()>
    where
        'a: 'b,
    {
        let elastic = (*map.get(&TypeId::of::<T>()).unwrap().inner)
            .downcast_ref::<Elastic<StretchedMut<T>>>()
            .unwrap();
        guard.loan(elastic, &mut **self);

        Ok(())
    }
}

impl<'a, U, T: 'static + AlchemicalAny> ErasedLoan<'a> for ArcRef<T, U> {
    fn erased_loan<'b>(
        &'b mut self,
        guard: &mut ScopeGuard<'b>,
        map: &HashMap<TypeId, ScriptResource>,
    ) -> Result<()>
    where
        'a: 'b,
    {
        let elastic = (*map.get(&TypeId::of::<T>()).unwrap().inner)
            .downcast_ref::<Elastic<StretchedRef<T>>>()
            .unwrap();
        guard.loan(elastic, &**self);

        Ok(())
    }
}

impl<'a, U, T: 'static + AlchemicalAny> ErasedLoan<'a> for ArcRefMut<T, U> {
    fn erased_loan<'b>(
        &'b mut self,
        guard: &mut ScopeGuard<'b>,
        map: &HashMap<TypeId, ScriptResource>,
    ) -> Result<()>
    where
        'a: 'b,
    {
        let elastic = (*map.get(&TypeId::of::<T>()).unwrap().inner)
            .downcast_ref::<Elastic<StretchedMut<T>>>()
            .unwrap();
        guard.loan(elastic, &mut **self);

        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScriptResourceSetBuilder {
    types: HashSet<TypeId>,
}

impl ScriptResourceSetBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add<T: 'static + Send + Sync>(&mut self) -> &mut Self {
        self.types.insert(TypeId::of::<T>());
        self
    }

    pub fn build(&mut self) -> ScriptResourceSet {
        ScriptResourceSet {
            types: self.types.drain().collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScriptResourceSet {
    types: Box<[TypeId]>,
}

/// A resource context for executing Lua scripts.
///
/// `ScriptContext` provides functionality for automatically handling "loaning" of references from a
/// [`Resources`] registry into Lua. It does this by keeping track of a number of [`Elastic`]s and
/// stretching the lifetimes of inserted resources such that they can be temporarily lent as Lua
/// userdata. There are four main ways you can register a resource w/ a `ScriptContext`:
///
/// - Mutably and directly; when the `ScriptContext` needs this value, it will call
///   [`Resources::get_mut`] on the [`Resources`], and loan the reference from the resulting
///   [`RefMut`](resources::RefMut) to Lua, while holding on to the [`RefMut`](resources::RefMut)
///   for the lifetime of the borrow to ensure that the reference remains valid.
/// - Immutably and directly; just like the mutable approach, but uses [`Resources::get`] and
///   [`Ref`](resources::Ref) instea dof their mutable counterparts. The immutable access methods
///   have a fairly limited use case, as they don't allow Lua to use userdata methods which are
///   marked mutable; an attempt to use them will cause an error.
/// - Mutably and indirectly/reborrowed. This is what you need when you can't actually have your
///   [`Resources`] own the value in question; the mutable approach looks for an
///   [`Elastic<StretchedMut<T>>`] in the [`Resources`] instead, and uses
///   [`Elastic::borrow_arc_mut`] to extract an [`ArcRefMut`] which takes the place of the
///   [`RefMut`](resources::RefMut) guard.
/// - Immutably and indirectly/reborrowed, just like the mutable/indirect case but using [`ArcRef`]
///   for an immutable-only access scheme.
///
/// That's quite a few. In addition to that, there are three ways each that you can actually
/// register the type:
///
/// - The most basic, the `insert_` family, allows you to insert your own [`Elastic`] in the case
///   that you already have one that you want this `ScriptContext` to loan to.
/// - The `register_` family creates the [`Elastic`] for you and then returns it so you can put it
///   wherever it needs to be.
/// - The `set_` family is the most convenient, takes a reference to the Lua context and a string
///   key, and registers the [`Elastic`] while immediately inserting it into the script context's
///   environment table. Useful for when you just need to shove something into Lua quickly and don't
///   care about getting a really careful API going.
#[derive(Debug)]
pub struct ScriptContext {
    scope_arena: ScopeArena,
    bump_arena: Bump,
    map: HashMap<TypeId, ScriptResource>,
    env: LuaRegistryKey,
}

impl ScriptContext {
    /// Create a new script context with an associated environment table.
    pub fn new(env: LuaRegistryKey) -> Self {
        Self {
            scope_arena: ScopeArena::new(),
            bump_arena: Bump::new(),
            map: HashMap::new(),
            env,
        }
    }

    /// Set the environment table, returning the old registry key value.
    pub fn set_env_key(&mut self, env: LuaRegistryKey) -> LuaRegistryKey {
        std::mem::replace(&mut self.env, env)
    }

    /// Get a reference to the environment table registry key.
    pub fn env_key(&self) -> &LuaRegistryKey {
        &self.env
    }

    /// Register a resource type as being an immutably accessible resource.
    ///
    /// Panics if this resource is already registered in the context, whether as mutable or
    /// immutable.
    ///
    /// Allowing resources to be registered as mutable or immutable (and flexibly choosing which
    /// depending on the resource set/resources offered) is to-do, but not currently implemented.
    pub fn insert_ref<T: LuaUserData + Send + Sync + 'static>(
        &mut self,
        elastic: Elastic<StretchedRef<T>>,
    ) {
        let replaced = self
            .map
            .insert(TypeId::of::<T>(), ScriptResource::with_ref::<T>(elastic));
        assert!(
            replaced.is_none(),
            "already a resource of this type in the map!"
        );
    }

    /// Register a resource type as being a mutably accessible resource.
    ///
    /// Panics if this resource is already registered in the context, whether as mutable or
    /// immutable.
    ///
    /// Allowing resources to be registered as mutable or immutable (and flexibly choosing which
    /// depending on the resource set/resources offered) is to-do, but not currently implemented.
    pub fn insert_mut<T: LuaUserData + Send + Sync + 'static>(
        &mut self,
        elastic: Elastic<StretchedMut<T>>,
    ) {
        let replaced = self
            .map
            .insert(TypeId::of::<T>(), ScriptResource::with_mut::<T>(elastic));
        assert!(
            replaced.is_none(),
            "already a resource of this type in the map!"
        );
    }

    /// Register a resource type as being an immutably accessible resource, which must be immutably
    /// re-borrowed from a stretched type.
    ///
    /// Panics if this resource is already registered in the context, whether as mutable or
    /// immutable.
    ///
    /// Allowing resources to be registered as mutable or immutable (and flexibly choosing which
    /// depending on the resource set/resources offered) is to-do, but not currently implemented.
    pub fn insert_reborrowed_ref<T: LuaUserData + Send + Sync + 'static>(
        &mut self,
        elastic: Elastic<StretchedRef<T>>,
    ) {
        let replaced = self.map.insert(
            TypeId::of::<T>(),
            ScriptResource::with_arc_ref::<T>(elastic),
        );
        assert!(
            replaced.is_none(),
            "already a resource of this type in the map!"
        );
    }

    /// Register a resource type as being a mutably accessible resource, which must be mutably
    /// re-borrowed from a stretched type.
    ///
    /// Panics if this resource is already registered in the context, whether as mutable or
    /// immutable.
    ///
    /// Allowing resources to be registered as mutable or immutable (and flexibly choosing which
    /// depending on the resource set/resources offered) is to-do, but not currently implemented.
    pub fn insert_reborrowed_mut<T: LuaUserData + Send + Sync + 'static>(
        &mut self,
        elastic: Elastic<StretchedMut<T>>,
    ) {
        let replaced = self.map.insert(
            TypeId::of::<T>(),
            ScriptResource::with_arc_mut::<T>(elastic),
        );
        assert!(
            replaced.is_none(),
            "already a resource of this type in the map!"
        );
    }

    /// Register a resource type as being an immutably accessible resource, returning the registered
    /// elastic.
    ///
    /// Panics if this resource is already registered in the context, whether as mutable or
    /// immutable.
    ///
    /// Allowing resources to be registered as mutable or immutable (and flexibly choosing which
    /// depending on the resource set/resources offered) is to-do, but not currently implemented.
    pub fn register_ref<T: LuaUserData + Send + Sync + 'static>(
        &mut self,
    ) -> Elastic<StretchedRef<T>> {
        let elastic = Elastic::new();
        self.insert_ref(elastic.clone());
        elastic
    }

    /// Register a resource type as being mutably accessible resource, returning the registered
    /// elastic.
    ///
    /// Panics if this resource is already registered in the context, whether as mutable or
    /// immutable.
    ///
    /// Allowing resources to be registered as mutable or immutable (and flexibly choosing which
    /// depending on the resource set/resources offered) is to-do, but not currently implemented.
    pub fn register_mut<T: LuaUserData + Send + Sync + 'static>(
        &mut self,
    ) -> Elastic<StretchedMut<T>> {
        let elastic = Elastic::new();
        self.insert_mut(elastic.clone());
        elastic
    }

    /// Register a resource type as being a re-borrowed immutably accessible resource, returning the
    /// registered elastic.
    ///
    /// Panics if this resource is already registered in the context, whether as mutable or
    /// immutable.
    ///
    /// Allowing resources to be registered as mutable or immutable (and flexibly choosing which
    /// depending on the resource set/resources offered) is to-do, but not currently implemented.
    pub fn register_reborrowed_ref<T: LuaUserData + Send + Sync + 'static>(
        &mut self,
    ) -> Elastic<StretchedRef<T>> {
        let elastic = Elastic::new();
        self.insert_reborrowed_ref(elastic.clone());
        elastic
    }

    /// Register a resource type as being a re-borrowed mutably accessible resource, returning the
    /// registered elastic.
    ///
    /// Panics if this resource is already registered in the context, whether as mutable or
    /// immutable.
    ///
    /// Allowing resources to be registered as mutable or immutable (and flexibly choosing which
    /// depending on the resource set/resources offered) is to-do, but not currently implemented.
    pub fn register_reborrowed_mut<T: LuaUserData + Send + Sync + 'static>(
        &mut self,
    ) -> Elastic<StretchedMut<T>> {
        let elastic = Elastic::new();
        self.insert_reborrowed_mut(elastic.clone());
        elastic
    }

    /// Register a resource type as being immutably accessible and insert it into the value at the
    /// key `name` in the environment table.
    ///
    /// Panics if this context has no environment table.
    pub fn set_ref<T: LuaUserData + Send + Sync + 'static>(
        &mut self,
        lua: &Lua,
        name: &str,
    ) -> Result<()> {
        let elastic = self.register_ref::<T>();
        let table: LuaTable = lua.registry_value(&self.env)?;
        table.set(name, elastic)?;
        Ok(())
    }

    /// Register a resource type as being mutably accessible and insert it into the value at the key
    /// `name` in the environment table.
    ///
    /// Panics if this context has no environment table.
    pub fn set_mut<T: LuaUserData + Send + Sync + 'static>(
        &mut self,
        lua: &Lua,
        name: &str,
    ) -> Result<()> {
        let elastic = self.register_mut::<T>();
        let table: LuaTable = lua.registry_value(&self.env)?;
        table.set(name, elastic)?;
        Ok(())
    }

    /// Register a resource type as being immutably accessible via a re-borrow and insert it into
    /// the value at the key `name` in the environment table.
    ///
    /// Panics if this context has no environment table.
    pub fn set_reborrowed_ref<T: LuaUserData + Send + Sync + 'static>(
        &mut self,
        lua: &Lua,
        name: &str,
    ) -> Result<()> {
        let elastic = self.register_reborrowed_ref::<T>();
        let table: LuaTable = lua.registry_value(&self.env)?;
        table.set(name, elastic)?;
        Ok(())
    }

    /// Register a resource type as being mutably accessible via a re-borrow and insert it into the
    /// value at the key `name` in the environment table.
    ///
    /// Panics if this context has no environment table.
    pub fn set_reborrowed_mut<T: LuaUserData + Send + Sync + 'static>(
        &mut self,
        lua: &Lua,
        name: &str,
    ) -> Result<()> {
        let elastic = self.register_reborrowed_mut::<T>();
        let table: LuaTable = lua.registry_value(&self.env)?;
        table.set(name, elastic)?;
        Ok(())
    }

    /// Borrow a set of resources from a [`Resources`] and loan it out to matching resources
    /// registered with this context, and then call the given thunk, passing in the environment
    /// table of this script context (if it exists.)
    ///
    /// Loans will be valid until the thunk returns.
    pub fn with_resources_from_set<'lua, R>(
        &mut self,
        lua: &'lua Lua,
        resource_set: &ScriptResourceSet,
        resources: &Resources,
        f: impl FnOnce(LuaTable<'lua>) -> R,
    ) -> Result<R> {
        let mut erased_loanables = Vec::with_capacity_in(self.map.len(), &self.bump_arena);
        for ty in resource_set.types.iter() {
            let resource = match self.map.get(ty) {
                Some(resource) => resource,
                None => {
                    // TODO: warn about this case. What we have here is, a resource was in the set
                    // requested to be borrowed, *but*, the `ScriptContext` doesn't need it/doesn't
                    // know where to put it.
                    continue;
                }
            };

            match (resource.loanable)(resources, &self.bump_arena) {
                Ok(loanable) => erased_loanables.push(loanable),
                Err(error) => tracing::error!(
                    name = %resource.name,
                    ?error,
                    "failed to borrow resource {}: {:?}",
                    resource.name,
                    error,
                ),
            }
        }

        let out = self.scope_arena.scope(|guard| {
            for loanable in &mut erased_loanables {
                loanable.erased_loan(guard, &self.map)?;
            }

            Ok(f(lua.registry_value(&self.env)?))
        });

        drop(erased_loanables);

        self.bump_arena.reset();
        self.scope_arena.reset();

        out
    }

    /// Borrow all of the registered resources in this context from the given [`Resources`], and
    /// then call the given thunk, passing in the registry key corresponding to this script
    /// context's environment table (if it exists.)
    ///
    /// Loans will be valid until the thunk returns.
    pub fn with_resources<'lua, R>(
        &mut self,
        lua: &'lua Lua,
        resources: &Resources,
        f: impl FnOnce(LuaTable<'lua>) -> R,
    ) -> Result<R> {
        let mut erased_loanables = Vec::with_capacity_in(self.map.len(), &self.bump_arena);
        for resource in self.map.values() {
            match (resource.loanable)(resources, &self.bump_arena) {
                Ok(loanable) => erased_loanables.push(loanable),
                Err(error) => tracing::error!(
                    name = %resource.name,
                    ?error,
                    "failed to borrow resource {}: {:?}",
                    resource.name,
                    error,
                ),
            }
        }

        let out = self.scope_arena.scope(|guard| {
            for loanable in &mut erased_loanables {
                loanable.erased_loan(guard, &self.map)?;
            }

            Ok(f(lua.registry_value(&self.env)?))
        });

        drop(erased_loanables);

        self.bump_arena.reset();
        self.scope_arena.reset();

        out
    }
}

// /// A type-indexed map which maps [`TypeId`]s to [`Elastic<StretchedMut<T>>`] and
// /// [`Elastic<StretchedRef<T>>`] for [`T: UserData`](LuaUserData), and provides Lua access to these
// /// stretched/loaned values.
// ///
// /// Values can be registered with this type using either [`ScriptResources::insert_ref`] or
// /// [`ScriptResources::insert_mut`]. Using either one is a *commitment* to how you plan to access
// /// the type in the future, and to how you expect scripts/Lua to access the type. In the future it
// /// may be possible to lift this requirement and dynamically check whether a type has been loaned by
// /// reference or by mutable reference, but not now.
// #[derive(Clone)]
// pub struct ScriptResources {
//     map: ArcCell<HashMap<TypeId, ScriptResource>>,
// }

// impl Default for ScriptResources {
//     fn default() -> Self {
//         Self::new()
//     }
// }

// impl ScriptResources {
//     /// Create an empty [`ScriptResources`] object, capable of loaning data to Lua.
//     pub fn new() -> Self {
//         Self {
//             map: ArcCell::default(),
//         }
//     }

//     /// Insert a resource which can be accessed immutably. Attempts to mutably access it on the Lua
//     /// side will cause an error. Loans to this type must be done using [`loan_ref`].
//     pub fn insert_ref<T: LuaUserData + Send + Sync + 'static>(&self) {
//         self.map
//             .as_inner()
//             .borrow_mut()
//             .insert(TypeId::of::<T>(), ScriptResource::new_ref::<T>())
//             .expect("already a resource of this type in the map!");
//     }

//     /// Insert a resource which can be accessed mutably. Loans to the entry for this type must be
//     /// done using [`loan_mut`].
//     pub fn insert_mut<T: LuaUserData + Send + Sync + 'static>(&self) {
//         self.map
//             .as_inner()
//             .borrow_mut()
//             .insert(TypeId::of::<T>(), ScriptResource::new_mut::<T>())
//             .expect("already a resource of this type in the map!");
//     }

//     /// Immutably loan a resource, receiving back a scope guard and a reference to the "stretched"
//     /// type.
//     ///
//     /// # Safety
//     ///
//     /// The returned `ElasticGuard` *must* be dropped by the end of its lifetime.
//     pub fn loan_ref<'a, T: LuaUserData + 'static>(
//         &self,
//         guard: &mut ScopeGuard<'a>,
//         val: &'a T,
//     ) -> Option<Elastic<StretchedRef<T>>> {
//         let elastic = self
//             .map
//             .as_inner()
//             .borrow()
//             .get(&TypeId::of::<T>())?
//             .inner
//             .downcast_ref::<Elastic<StretchedRef<T>>>()?
//             .clone();
//         guard.loan(&elastic, val);
//         Some(elastic)
//     }

//     /// Mutably loan a resource, receiving back a scope guard and a reference to the "stretched"
//     /// type.
//     ///
//     /// # Safety
//     ///
//     /// The returned `ElasticGuard` *must* be dropped by the end of its lifetime.
//     pub fn loan_mut<'a, T: LuaUserData + 'static>(
//         &self,
//         guard: &mut ScopeGuard<'a>,
//         val: &'a mut T,
//     ) -> Option<Elastic<StretchedMut<T>>> {
//         let elastic = self
//             .map
//             .as_inner()
//             .borrow()
//             .get(&TypeId::of::<T>())?
//             .inner
//             .downcast_ref::<Elastic<StretchedMut<T>>>()?
//             .clone();
//         guard.loan(&elastic, val);
//         Some(elastic)
//     }

//     /// Get a resource and dynamically convert it to userdata without requiring a static type.
//     pub fn get_userdata<'lua>(
//         &self,
//         lua: &'lua Lua,
//         type_id: TypeId,
//     ) -> Option<LuaAnyUserData<'lua>> {
//         Some(
//             self.map
//                 .as_inner()
//                 .borrow()
//                 .get(&type_id)?
//                 .inner
//                 .dyncast_ref::<dyn TryCloneToUserDataExt>()
//                 .expect("Elastic should always succeed dyncast to dyn TryCloneToUserData!")
//                 .try_clone_to_user_data(lua)
//                 .expect("Elastic should always succeed clone to userdata! ... unless it's empty"),
//         )
//     }
// }

// impl LuaUserData for ScriptResources {
//     fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
//         // Given a userdata `Type<T>` object, try to extract the corresponding resource and return
//         // it, converted to userdata.
//         methods.add_method("get", |lua, this, ud_type: LuaAnyUserData| {
//             Ok(ud_type
//                 .meta_type_table()
//                 .ok()
//                 .map(|type_table| this.get_userdata(lua, type_table.id)))
//         });
//     }
// }

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

// /// A resource type, representing a context in which to load and run scripts. Holds references to a
// /// [`ScriptResources`] object used to loan external resources into Lua as well as a Lua registry
// /// key referring to a Lua table used as the global environment table for Lua scripts.
// pub struct ScriptContext {
//     // Environment table for scripts loaded in this context
//     env: LuaRegistryKey,
//     resources: ScriptResources,
// }

// /// The string name of the Lua global which holds the [`ScriptResources`].
// pub const SCRIPT_RESOURCES: &str = "_RESOURCES";

// static_assertions::assert_impl_all!(ScriptContext: Send, Sync);

// impl ScriptContext {
//     /// Create a new script context w/ a parent environment table.
//     ///
//     /// The environment table is used as the `__index` of the metatable of a newly created table
//     /// which holds a reference to a [`ScriptResources`] object, with the string key
//     /// [`SCRIPT_RESOURCES`]. [`ScriptResources`] allows Lua scripts access to external resources
//     /// which are available for only a non-`'static` period; see the struct docs for more.
//     pub fn new(lua: &Lua, env_table: LuaTable) -> Result<Self> {
//         let env_mt = lua.create_table()?;
//         env_mt.set("__index", env_table)?;
//         let wrapped_env_table = lua.create_table()?;
//         wrapped_env_table.set_metatable(Some(env_mt));
//         let resources = ScriptResources::new();
//         wrapped_env_table.set("_RESOURCES", resources.clone())?;
//         let env = lua.create_registry_value(wrapped_env_table)?;
//         Ok(Self { env, resources })
//     }

//     /// Get the [`ScriptResources`] object.
//     pub fn resources(&self) -> &ScriptResources {
//         &self.resources
//     }

//     /// Get the environment table used by this [`ScriptContext`].
//     pub fn env(&self) -> &LuaRegistryKey {
//         &self.env
//     }
// }

// /// A local system which attempts to load new scripts from [`Script`] components and attach them as
// /// [`LuaRegistryKey`] components. Failing to load a script will cause a [`ScriptLoadError`]
// /// containing the error to be attached instead, and the error will also be logged at the `Error`
// /// level.
// pub fn script_upkeep_system(
//     context: SystemContext,
//     (scope_arena, script_context, fs, command_pool): (
//         &ScopeArena,
//         &mut ScriptContext,
//         &mut Filesystem,
//         &CommandPoolResource,
//     ),
//     lua: &Lua,
//     (unloaded_scripts,): (QueryMarker<Without<ScriptLoadError, Without<LuaRegistryKey, &Script>>>,),
// ) {
//     let _span = trace_span!("script_upkeep_system").entered();
//     scope_arena.scope(|scope| {
//         let fs = script_context
//             .resources
//             .loan_mut(scope, fs)
//             .expect("no filesystem resource!");
//         let mut command_buffer = command_pool.get_buffer();

//         let mut buf = String::new();
//         let env_table: LuaTable = lua.registry_value(&script_context.env).unwrap();

//         for (entity, script) in context.query(unloaded_scripts).iter() {
//             let res = (|| -> Result<LuaRegistryKey> {
//                 let mut mut_fs = fs.borrow_mut().unwrap();
//                 buf.clear();
//                 mut_fs.open(&script.path)?.read_to_string(&mut buf)?;
//                 drop(mut_fs);
//                 let loaded: LuaValue = lua
//                     .load(&buf)
//                     .set_name(&script.path)?
//                     .set_environment(env_table.clone())?
//                     .call(())?;
//                 let registry_key = lua.create_registry_value(loaded)?;
//                 Ok(registry_key)
//             })();

//             match res {
//                 Ok(key) => command_buffer.insert(entity, (key,)),
//                 Err(error) => {
//                     error!(
//                         ?entity,
//                         script = ?script.path,
//                         ?error,
//                         "error instantiating entity script: {:#}",
//                         error
//                     );

//                     command_buffer.insert(entity, (ScriptLoadError { error },));
//                 }
//             }
//         }
//     });
// }

// /// A local system which looks for entities with [`Script`] and [`LuaRegistryKey`] components;
// /// and tries to extract a table from the [`LuaRegistryKey`] component and run an `update` method on
// /// it if it exists. If calling the `update` method fails, an error will be logged.
// pub fn script_update_system(
//     context: SystemContext,
//     (scope_arena, script_context, fs, &Dt(dt)): (
//         &ScopeArena,
//         &mut ScriptContext,
//         &mut Filesystem,
//         &Dt,
//     ),
//     lua: &Lua,
//     (with_registry_key,): (QueryMarker<(&Script, &LuaRegistryKey)>,),
// ) {
//     let _span = trace_span!("script_update_system").entered();
//     scope_arena.scope(|scope| {
//         let _fs = script_context.resources.loan_mut(scope, fs);

//         for (entity, (script, key)) in context.query(with_registry_key).iter() {
//             let res = (|| -> Result<()> {
//                 let table = lua.registry_value::<LuaTable>(key)?;
//                 if table.contains_key("update")? {
//                     let _: () = table.call_method("update", (dt,))?;
//                 }
//                 Ok(())
//             })();

//             if let Err(err) = res {
//                 error!(
//                     entity = ?entity,
//                     script = ?script.path,
//                     error = ?err,
//                     "error calling entity script update: {:#}",
//                     err
//                 );
//             }
//         }
//     });
// }
