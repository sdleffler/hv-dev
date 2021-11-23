//! Components, systems, and resources, for loading, running/updating, and managing resources
//! provided to Lua scripts.

#![no_std]
#![feature(allocator_api)]

extern crate alloc;

use alloc::{borrow::ToOwned, boxed::Box, string::String, vec::Vec};
use anyhow::*;
use core::{
    any::{Any, TypeId},
    fmt,
};
use hashbrown::{HashMap, HashSet};

use hv_cell::{ArcRef, ArcRefMut};
use hv_elastic::{Elastic, ScopeArena, ScopeGuard, StretchedMut, StretchedRef};
use hv_lua::prelude::*;
use hv_resources::{self, Resources};
use hv_stampede::{boxed::Box as ArenaBox, Bump};

struct ScriptResource {
    inner: Box<dyn Any + Send + Sync>,
    #[allow(clippy::type_complexity)]
    loanable: Box<
        dyn for<'a, 'lua, 'g> Fn(
            &'lua Lua,
            &'a LuaTable<'lua>,
            &'g Resources,
            &'g Bump,
            &'a mut Vec<ArenaBox<'g, dyn ErasedLoan<'g> + 'g>, &'g Bump>,
        ) -> Result<()>,
    >,
    name: String,
}

impl fmt::Debug for ScriptResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ScriptResource {{ name: {} }}", self.name)
    }
}

impl ScriptResource {
    fn with_ref<T, F>(elastic: Elastic<StretchedRef<T>>, on_loan: F) -> Self
    where
        T: 'static + Send + Sync,
        F: for<'a, 'lua> Fn(&'a T, &'lua Lua, &'a LuaTable<'lua>) -> Result<()> + 'static,
    {
        fn loan_ref<'a, 'g, 'lua, T: 'static>(
            on_loan: &'a impl for<'t, 'lua2> Fn(&'t T, &'lua2 Lua, &'t LuaTable<'lua2>) -> Result<()>,
            lua: &'lua Lua,
            env: &'a LuaTable<'lua>,
            resources: &'g Resources,
            alloc: &'g Bump,
            loans: &'a mut Vec<ArenaBox<'g, dyn ErasedLoan<'g> + 'g>, &'g Bump>,
        ) -> Result<()> {
            let guard = resources.get::<T>()?;
            on_loan(&guard, lua, env)?;
            let owned = ArenaBox::new_in(guard, alloc);
            // We have to do a pointer cast here since `ArenaBox` can't deal with `CoerceUnsized`.
            let raw = ArenaBox::into_raw(owned) as *mut (dyn ErasedLoan<'g> + 'g);
            let owned = unsafe { <ArenaBox<'g, _>>::from_raw(raw) };
            loans.push(owned);
            Ok(())
        }

        Self {
            inner: Box::new(elastic),
            loanable: Box::new(move |lua, env, resources, alloc, loans| {
                loan_ref(&on_loan, lua, env, resources, alloc, loans)
            }),
            name: core::any::type_name::<T>().to_owned(),
        }
    }

    fn with_mut<T, F>(elastic: Elastic<StretchedMut<T>>, on_loan: F) -> Self
    where
        T: 'static + Send + Sync,
        F: for<'a, 'lua> Fn(&'a mut T, &'lua Lua, &'a LuaTable<'lua>) -> Result<()> + 'static,
    {
        fn loan_mut<'a, 'g, 'lua, T: 'static>(
            on_loan: &'a impl for<'t, 'lua2> Fn(
                &'t mut T,
                &'lua2 Lua,
                &'t LuaTable<'lua2>,
            ) -> Result<()>,
            lua: &'lua Lua,
            env: &'a LuaTable<'lua>,
            resources: &'g Resources,
            alloc: &'g Bump,
            loans: &'a mut Vec<ArenaBox<'g, dyn ErasedLoan<'g> + 'g>, &'g Bump>,
        ) -> Result<()> {
            let mut guard = resources.get_mut::<T>()?;
            on_loan(&mut guard, lua, env)?;
            let owned = ArenaBox::new_in(guard, alloc);
            // We have to do a pointer cast here since `ArenaBox` can't deal with `CoerceUnsized`.
            let raw = ArenaBox::into_raw(owned) as *mut (dyn ErasedLoan<'g> + 'g);
            let cast = unsafe { <ArenaBox<'g, _>>::from_raw(raw) };
            loans.push(cast);
            Ok(())
        }

        Self {
            inner: Box::new(elastic),
            loanable: Box::new(move |lua, env, resources, alloc, loans| {
                loan_mut(&on_loan, lua, env, resources, alloc, loans)
            }),
            name: core::any::type_name::<T>().to_owned(),
        }
    }

    fn with_arc_ref<T, F>(elastic: Elastic<StretchedRef<T>>, on_loan: F) -> Self
    where
        T: 'static + Send + Sync,
        F: for<'a, 'lua> Fn(&'a T, &'lua Lua, &'a LuaTable<'lua>) -> Result<()> + 'static,
    {
        fn loan_arc_ref<'a, 'g, 'lua, T: 'static>(
            on_loan: &'a impl for<'t, 'lua2> Fn(&'t T, &'lua2 Lua, &'t LuaTable<'lua2>) -> Result<()>,
            lua: &'lua Lua,
            env: &'a LuaTable<'lua>,
            resources: &'g Resources,
            alloc: &'g Bump,
            loans: &'a mut Vec<ArenaBox<'g, dyn ErasedLoan<'g> + 'g>, &'g Bump>,
        ) -> Result<()> {
            let guard = resources.get::<Elastic<StretchedRef<T>>>()?;
            let owned = ArenaBox::new_in(guard.borrow_arc(), alloc);
            on_loan(&owned, lua, env)?;
            // We have to do a pointer cast here since `ArenaBox` can't deal with `CoerceUnsized`.
            let raw = ArenaBox::into_raw(owned) as *mut (dyn ErasedLoan<'g> + 'g);
            let cast = unsafe { <ArenaBox<'g, dyn ErasedLoan<'g> + 'g>>::from_raw(raw) };
            loans.push(cast);
            Ok(())
        }

        Self {
            inner: Box::new(elastic),
            loanable: Box::new(move |lua, env, resources, alloc, loans| {
                loan_arc_ref(&on_loan, lua, env, resources, alloc, loans)
            }),
            name: core::any::type_name::<T>().to_owned(),
        }
    }

    fn with_arc_mut<T, F>(elastic: Elastic<StretchedMut<T>>, on_loan: F) -> Self
    where
        T: 'static + Send + Sync,
        F: for<'a, 'lua> Fn(&'a mut T, &'lua Lua, &'a LuaTable<'lua>) -> Result<()> + 'static,
    {
        fn loan_arc_mut<'a, 'g, 'lua, T: 'static>(
            on_loan: &'a impl for<'t, 'lua2> Fn(
                &'t mut T,
                &'lua2 Lua,
                &'t LuaTable<'lua2>,
            ) -> Result<()>,
            lua: &'lua Lua,
            env: &'a LuaTable<'lua>,
            resources: &'g Resources,
            alloc: &'g Bump,
            loans: &'a mut Vec<ArenaBox<'g, dyn ErasedLoan<'g> + 'g>, &'g Bump>,
        ) -> Result<()> {
            let guard = resources.get::<Elastic<StretchedMut<T>>>()?;
            let mut owned = ArenaBox::new_in(guard.borrow_arc_mut(), alloc);
            on_loan(&mut owned, lua, env)?;
            // We have to do a pointer cast here since `ArenaBox` can't deal with `CoerceUnsized`.
            let raw = ArenaBox::into_raw(owned) as *mut (dyn ErasedLoan<'g> + 'g);
            let owned = unsafe { <ArenaBox<'g, _>>::from_raw(raw) };
            loans.push(owned);
            Ok(())
        }

        Self {
            inner: Box::new(elastic),
            loanable: Box::new(move |lua, env, resources, alloc, loans| {
                loan_arc_mut(&on_loan, lua, env, resources, alloc, loans)
            }),
            name: core::any::type_name::<T>().to_owned(),
        }
    }
}

trait ErasedLoan<'a>: 'a {
    fn erased_loan<'b>(
        &'b mut self,
        guard: &mut ScopeGuard<'b>,
        map: &HashMap<TypeId, ScriptResource>,
    ) -> Result<()>;
}

impl<'a, T: 'static + Any> ErasedLoan<'a> for hv_resources::Ref<'a, T> {
    fn erased_loan<'b>(
        &'b mut self,
        guard: &mut ScopeGuard<'b>,
        map: &HashMap<TypeId, ScriptResource>,
    ) -> Result<()> {
        let elastic = (*map.get(&TypeId::of::<T>()).unwrap().inner)
            .downcast_ref::<Elastic<StretchedRef<T>>>()
            .unwrap();
        guard.loan(elastic, &**self);

        Ok(())
    }
}

impl<'a, T: 'static + Any> ErasedLoan<'a> for hv_resources::RefMut<'a, T> {
    fn erased_loan<'b>(
        &'b mut self,
        guard: &mut ScopeGuard<'b>,
        map: &HashMap<TypeId, ScriptResource>,
    ) -> Result<()> {
        let elastic = (*map.get(&TypeId::of::<T>()).unwrap().inner)
            .downcast_ref::<Elastic<StretchedMut<T>>>()
            .unwrap();
        guard.loan(elastic, &mut **self);

        Ok(())
    }
}

impl<'a, U: 'static, T: 'static + Any> ErasedLoan<'a> for ArcRef<T, U> {
    fn erased_loan<'b>(
        &'b mut self,
        guard: &mut ScopeGuard<'b>,
        map: &HashMap<TypeId, ScriptResource>,
    ) -> Result<()> {
        let elastic = (*map.get(&TypeId::of::<T>()).unwrap().inner)
            .downcast_ref::<Elastic<StretchedRef<T>>>()
            .unwrap();
        guard.loan(elastic, &**self);

        Ok(())
    }
}

impl<'a, U: 'static, T: 'static + Any> ErasedLoan<'a> for ArcRefMut<T, U> {
    fn erased_loan<'b>(
        &'b mut self,
        guard: &mut ScopeGuard<'b>,
        map: &HashMap<TypeId, ScriptResource>,
    ) -> Result<()> {
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
///
/// Wow! That's 12 ways to register a type. Oh wait, there's more! Every single one of those ways
/// has an additional `_with_callback` version. Under the hood, they call out to their
/// `_with_callback` counterpart with an empty callback (`|_, _, _| Ok(())`.) This callback allows
/// you to supply resources taken from the script context's environment table into a given resource
/// when it is loaned to the context. This is useful for, say, if you have a reborrowed `World` and
/// also a `Camera` resource, and you want to give that `Camera` an `Elastic<StretchedMut<World>>`
/// so that you can tell it to look at an entity instead of a point; in this "on loan callback", you
/// can do something like `|this, _lua, env| { this.world = env.get("world")?; Ok(()) }`, and then
/// you'll be able to borrow the loaned-in world from inside your `Camera`, maybe even in a
/// `LuaUserData` impl.
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
        core::mem::replace(&mut self.env, env)
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
        self.insert_ref_with_callback(elastic, |_, _, _| Ok(()));
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
        self.insert_mut_with_callback(elastic, |_, _, _| Ok(()));
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
        self.insert_reborrowed_ref_with_callback(elastic, |_, _, _| Ok(()));
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
        self.insert_reborrowed_mut_with_callback(elastic, |_, _, _| Ok(()));
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

    /// `insert_ref` with an on-loan callback.
    pub fn insert_ref_with_callback<T, F>(&mut self, elastic: Elastic<StretchedRef<T>>, on_loan: F)
    where
        T: LuaUserData + Send + Sync + 'static,
        F: for<'a, 'lua> Fn(&'a T, &'lua Lua, &'a LuaTable<'lua>) -> Result<()> + 'static,
    {
        let replaced = self.map.insert(
            TypeId::of::<T>(),
            ScriptResource::with_ref(elastic, on_loan),
        );
        assert!(
            replaced.is_none(),
            "already a resource of this type in the map!"
        );
    }

    /// `insert_mut` with an on-loan callback.
    pub fn insert_mut_with_callback<T, F>(&mut self, elastic: Elastic<StretchedMut<T>>, on_loan: F)
    where
        T: LuaUserData + Send + Sync + 'static,
        F: for<'a, 'lua> Fn(&'a mut T, &'lua Lua, &'a LuaTable<'lua>) -> Result<()> + 'static,
    {
        let replaced = self.map.insert(
            TypeId::of::<T>(),
            ScriptResource::with_mut(elastic, on_loan),
        );
        assert!(
            replaced.is_none(),
            "already a resource of this type in the map!"
        );
    }

    /// `insert_reborrowed_ref` with an on-loan callback.
    pub fn insert_reborrowed_ref_with_callback<T, F>(
        &mut self,
        elastic: Elastic<StretchedRef<T>>,
        on_loan: F,
    ) where
        T: LuaUserData + Send + Sync + 'static,
        F: for<'a, 'lua> Fn(&'a T, &'lua Lua, &'a LuaTable<'lua>) -> Result<()> + 'static,
    {
        let replaced = self.map.insert(
            TypeId::of::<T>(),
            ScriptResource::with_arc_ref(elastic, on_loan),
        );
        assert!(
            replaced.is_none(),
            "already a resource of this type in the map!"
        );
    }

    /// `insert_reborrowed_mut` with an on-loan callback.
    pub fn insert_reborrowed_mut_with_callback<T, F>(
        &mut self,
        elastic: Elastic<StretchedMut<T>>,
        on_loan: F,
    ) where
        T: LuaUserData + Send + Sync + 'static,
        F: for<'a, 'lua> Fn(&'a mut T, &'lua Lua, &'a LuaTable<'lua>) -> Result<()> + 'static,
    {
        let replaced = self.map.insert(
            TypeId::of::<T>(),
            ScriptResource::with_arc_mut(elastic, on_loan),
        );
        assert!(
            replaced.is_none(),
            "already a resource of this type in the map!"
        );
    }

    /// `register_ref` with an on-loan callback.
    pub fn register_ref_with_callback<T, F>(&mut self, on_loan: F) -> Elastic<StretchedRef<T>>
    where
        T: LuaUserData + Send + Sync + 'static,
        F: for<'a, 'lua> Fn(&'a T, &'lua Lua, &'a LuaTable<'lua>) -> Result<()> + 'static,
    {
        let elastic = Elastic::new();
        self.insert_ref_with_callback(elastic.clone(), on_loan);
        elastic
    }

    /// `register_mut` with an on-loan callback.
    pub fn register_mut_with_callback<T, F>(&mut self, on_loan: F) -> Elastic<StretchedMut<T>>
    where
        T: LuaUserData + Send + Sync + 'static,
        F: for<'a, 'lua> Fn(&'a mut T, &'lua Lua, &'a LuaTable<'lua>) -> Result<()> + 'static,
    {
        let elastic = Elastic::new();
        self.insert_mut_with_callback(elastic.clone(), on_loan);
        elastic
    }

    /// `register_reborrowed_ref` with an on-loan callback.
    pub fn register_reborrowed_ref_with_callback<T, F>(
        &mut self,
        on_loan: F,
    ) -> Elastic<StretchedRef<T>>
    where
        T: LuaUserData + Send + Sync + 'static,
        F: for<'a, 'lua> Fn(&'a T, &'lua Lua, &'a LuaTable<'lua>) -> Result<()> + 'static,
    {
        let elastic = Elastic::new();
        self.insert_reborrowed_ref_with_callback(elastic.clone(), on_loan);
        elastic
    }

    /// `register_reborrowed_mut` with an on-loan callback.
    pub fn register_reborrowed_mut_with_callback<T, F>(
        &mut self,
        on_loan: F,
    ) -> Elastic<StretchedMut<T>>
    where
        T: LuaUserData + Send + Sync + 'static,
        F: for<'a, 'lua> Fn(&'a mut T, &'lua Lua, &'a LuaTable<'lua>) -> Result<()> + 'static,
    {
        let elastic = Elastic::new();
        self.insert_reborrowed_mut_with_callback(elastic.clone(), on_loan);
        elastic
    }

    /// `set_ref` with an on-loan callback.
    pub fn set_ref_with_callback<T, F>(&mut self, lua: &Lua, name: &str, on_loan: F) -> Result<()>
    where
        T: LuaUserData + Send + Sync + 'static,
        F: for<'a, 'lua> Fn(&'a T, &'lua Lua, &'a LuaTable<'lua>) -> Result<()> + 'static,
    {
        let elastic = self.register_ref_with_callback(on_loan);
        let table: LuaTable = lua.registry_value(&self.env)?;
        table.set(name, elastic)?;
        Ok(())
    }

    /// `set_mut` with an on-loan callback.
    pub fn set_mut_with_callback<T, F>(&mut self, lua: &Lua, name: &str, on_loan: F) -> Result<()>
    where
        T: LuaUserData + Send + Sync + 'static,
        F: for<'a, 'lua> Fn(&'a mut T, &'lua Lua, &'a LuaTable<'lua>) -> Result<()> + 'static,
    {
        let elastic = self.register_mut_with_callback(on_loan);
        let table: LuaTable = lua.registry_value(&self.env)?;
        table.set(name, elastic)?;
        Ok(())
    }

    /// `set_reborrowed_ref` with an on-loan callback.
    pub fn set_reborrowed_ref_with_callback<T, F>(
        &mut self,
        lua: &Lua,
        name: &str,
        on_loan: F,
    ) -> Result<()>
    where
        T: LuaUserData + Send + Sync + 'static,
        F: for<'a, 'lua> Fn(&'a T, &'lua Lua, &'a LuaTable<'lua>) -> Result<()> + 'static,
    {
        let elastic = self.register_reborrowed_ref_with_callback(on_loan);
        let table: LuaTable = lua.registry_value(&self.env)?;
        table.set(name, elastic)?;
        Ok(())
    }

    /// `set_reborrowed_mut` with an on-loan callback.
    pub fn set_reborrowed_mut_with_callback<T, F>(
        &mut self,
        lua: &Lua,
        name: &str,
        on_loan: F,
    ) -> Result<()>
    where
        T: LuaUserData + Send + Sync + 'static,
        F: for<'a, 'lua> Fn(&'a mut T, &'lua Lua, &'a LuaTable<'lua>) -> Result<()> + 'static,
    {
        let elastic = self.register_reborrowed_mut_with_callback(on_loan);
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
        let env = lua.registry_value(&self.env)?;
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

            if let Err(error) = (resource.loanable)(
                lua,
                &env,
                resources,
                &self.bump_arena,
                &mut erased_loanables,
            ) {
                tracing::error!(
                    name = %resource.name,
                    ?error,
                    "failed to borrow resource {}: {:?}",
                    resource.name,
                    error,
                );
            }
        }

        let out = self.scope_arena.scope(|guard| {
            for loanable in &mut erased_loanables {
                loanable.erased_loan(guard, &self.map)?;
            }

            Ok(f(env))
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
        let env = lua.registry_value(&self.env)?;
        let mut erased_loanables = Vec::with_capacity_in(self.map.len(), &self.bump_arena);
        for resource in self.map.values() {
            if let Err(error) = (resource.loanable)(
                lua,
                &env,
                resources,
                &self.bump_arena,
                &mut erased_loanables,
            ) {
                tracing::error!(
                    name = %resource.name,
                    ?error,
                    "failed to borrow resource {}: {:?}",
                    resource.name,
                    error,
                );
            }
        }

        let out = self.scope_arena.scope(|guard| {
            for loanable in &mut erased_loanables {
                loanable.erased_loan(guard, &self.map)?;
            }

            Ok(f(env))
        });

        drop(erased_loanables);

        self.bump_arena.reset();
        self.scope_arena.reset();

        out
    }
}
