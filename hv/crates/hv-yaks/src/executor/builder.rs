use std::{collections::HashMap, fmt::Debug, hash::Hash};

#[cfg(feature = "parallel")]
use hv_ecs::World;

use super::SystemClosure;
use crate::{
    executor::LocalSystemClosure, Executor, Fetch, QueryBundle, ResourceTuple, SystemContext,
    SystemId,
};

#[cfg(feature = "parallel")]
use crate::{ArchetypeSet, BorrowSet, BorrowTypeSet, TypeSet};

pub enum BoxedSystemClosure<'closure, Resources, LocalResources>
where
    Resources: ResourceTuple + 'closure,
    Resources::Wrapped: Send + Sync,
    Resources::BorrowTuple: Send + Sync,
    LocalResources: ResourceTuple + 'closure,
{
    Sync(Box<SystemClosure<'closure, Resources::Wrapped>>),
    Local(Box<LocalSystemClosure<'closure, Resources::Wrapped, LocalResources::Wrapped>>),
}

/// Container for parsed systems and their metadata;
/// destructured in concrete executors' build functions.
pub struct System<'closure, Resources, LocalResources>
where
    Resources: ResourceTuple + 'closure,
    Resources::Wrapped: Send + Sync,
    Resources::BorrowTuple: Send + Sync,
    LocalResources: ResourceTuple + 'closure,
{
    pub closure: BoxedSystemClosure<'closure, Resources, LocalResources>,
    pub dependencies: Vec<SystemId>,
    #[cfg(feature = "parallel")]
    pub resource_set: BorrowSet,
    #[cfg(feature = "parallel")]
    pub component_type_set: BorrowTypeSet,
    #[cfg(feature = "parallel")]
    pub archetype_writer: Box<dyn Fn(&World, &mut ArchetypeSet) + Send>,
}

/// A builder for [`Executor`](struct.Executor.html) (and the only way of creating one).
pub struct ExecutorBuilder<'closures, Resources, LocalResources, Handle = DummyHandle>
where
    Resources: ResourceTuple,
    Resources::Wrapped: Send + Sync,
    Resources::BorrowTuple: Send + Sync,
    LocalResources: ResourceTuple,
{
    pub(crate) systems: HashMap<SystemId, System<'closures, Resources, LocalResources>>,
    pub(crate) handles: HashMap<Handle, SystemId>,
    #[cfg(feature = "parallel")]
    pub(crate) all_component_types: TypeSet,
}

impl<'closures, Resources, LocalResources, Handle>
    ExecutorBuilder<'closures, Resources, LocalResources, Handle>
where
    Resources: ResourceTuple,
    Resources::Wrapped: Send + Sync,
    Resources::BorrowTuple: Send + Sync,
    LocalResources: ResourceTuple,
    Handle: Eq + Hash,
{
    fn box_system<'a, Closure, ResourceRefs, Queries, Markers>(
        mut closure: Closure,
    ) -> System<'closures, Resources, LocalResources>
    where
        Resources::Wrapped: 'a,
        Closure: FnMut(SystemContext<'a>, ResourceRefs, &mut Queries) + Send + Sync + 'closures,
        ResourceRefs: Fetch<'a, Resources::Wrapped, Markers> + 'a,
        Queries: QueryBundle + Send + Sync + 'closures,
    {
        let mut queries = Queries::markers();
        let closure = Box::new(
            move |context: SystemContext<'a>, resources: &'a Resources::Wrapped| {
                let fetched = ResourceRefs::fetch(resources);
                closure(context, fetched, &mut queries);
                unsafe { ResourceRefs::release(resources) };
            },
        );
        let closure = unsafe {
            std::mem::transmute::<
                Box<dyn FnMut(_, &'a _) + Send + Sync + 'closures>,
                Box<dyn FnMut(SystemContext, &Resources::Wrapped) + Send + Sync + 'closures>,
            >(closure)
        };
        #[cfg(feature = "parallel")]
        {
            let mut resource_set = BorrowSet::with_capacity(Resources::LENGTH);
            ResourceRefs::set_resource_bits(&mut resource_set);
            let mut component_type_set = BorrowTypeSet::new();
            Queries::insert_component_types(&mut component_type_set);
            let archetype_writer = Box::new(|world: &World, archetype_set: &mut ArchetypeSet| {
                Queries::set_archetype_bits(world, archetype_set)
            });
            System {
                closure: BoxedSystemClosure::Sync(closure),
                dependencies: vec![],
                resource_set,
                component_type_set,
                archetype_writer,
            }
        }
        #[cfg(not(feature = "parallel"))]
        System {
            closure: BoxedSystemClosure::Sync(closure),
            dependencies: vec![],
        }
    }

    fn box_local_system<
        'a,
        Closure,
        ResourceRefs,
        LocalResourceRefs,
        Queries,
        Markers,
        LocalMarkers,
    >(
        mut closure: Closure,
    ) -> System<'closures, Resources, LocalResources>
    where
        Resources::Wrapped: 'a,
        LocalResources::Wrapped: 'a,
        Closure:
            FnMut(SystemContext<'a>, ResourceRefs, LocalResourceRefs, &mut Queries) + 'closures,
        ResourceRefs: Fetch<'a, Resources::Wrapped, Markers> + 'a,
        LocalResourceRefs: Fetch<'a, LocalResources::Wrapped, LocalMarkers> + 'a,
        Queries: QueryBundle + 'closures,
    {
        let mut queries = Queries::markers();
        let closure = Box::new(
            move |context: SystemContext<'a>,
                  resources: &'a Resources::Wrapped,
                  local_resources: &'a LocalResources::Wrapped| {
                let fetched = ResourceRefs::fetch(resources);
                let local_fetched = LocalResourceRefs::fetch(local_resources);
                closure(context, fetched, local_fetched, &mut queries);
                unsafe { ResourceRefs::release(resources) };
            },
        );
        let closure = unsafe {
            std::mem::transmute::<
                Box<dyn FnMut(_, &'a _, &'a _) + 'closures>,
                Box<
                    dyn FnMut(SystemContext, &Resources::Wrapped, &LocalResources::Wrapped)
                        + 'closures,
                >,
            >(closure)
        };
        #[cfg(feature = "parallel")]
        {
            let mut resource_set = BorrowSet::with_capacity(Resources::LENGTH);
            ResourceRefs::set_resource_bits(&mut resource_set);
            let mut component_type_set = BorrowTypeSet::new();
            Queries::insert_component_types(&mut component_type_set);
            let archetype_writer = Box::new(|world: &World, archetype_set: &mut ArchetypeSet| {
                Queries::set_archetype_bits(world, archetype_set)
            });
            System {
                closure: BoxedSystemClosure::Local(closure),
                dependencies: vec![],
                resource_set,
                component_type_set,
                archetype_writer,
            }
        }
        #[cfg(not(feature = "parallel"))]
        System {
            closure: BoxedSystemClosure::Local(closure),
            dependencies: vec![],
        }
    }

    /// Creates a new system from a closure or a function, and inserts it into the builder.
    ///
    /// The system-to-be must return nothing and have these 3 arguments:
    /// - [`SystemContext`](struct.SystemContext.html),
    /// - any tuple (up to 16) or a single one of "resources": references or mutable references
    /// to `Send + Sync` values not contained in a [`hv_ecs::World`](../hv-ecs/struct.World.html)
    /// that the system will be accessing,
    /// - any tuple (up to 16) or a single one of [`QueryMarker`](struct.QueryMarker.html) that
    /// represent the queries the system will be making.
    ///
    /// Additionally, closures may mutably borrow from their environment for the lifetime
    /// of the executor, but must be `Send + Sync`.
    ///
    /// All resources the system requires must correspond to a type in the executor's
    /// signature; e.g., if any number of systems require a `&f32` or a `&mut f32`,
    /// executor's generic parameter must contain `f32`.
    ///
    /// # Example
    /// ```rust
    /// # use hv_yaks::{QueryMarker, SystemContext, Executor};
    /// # let world = hv_ecs::World::new();
    /// # struct A;
    /// # struct B;
    /// # struct C;
    /// fn system_0(
    ///     context: SystemContext,
    ///     res_a: &A,
    ///     &mut (query_0, query_1): &mut (
    ///         QueryMarker<(&B, &mut C)>,
    ///         QueryMarker<hv_ecs::Without<B, &C>>
    ///     ),
    /// ) {
    ///     // This system may read resource of type `A`, and may prepare & execute queries
    ///     // of `(&B, &mut C)` and `hv_ecs::Without<B, &C>`.
    /// }
    ///
    /// fn system_1(
    ///     context: SystemContext,
    ///     (res_a, res_b): (&mut A, &B),
    ///     &mut query_0: &mut QueryMarker<(&mut B, &mut C)>,
    /// ) {
    ///     // This system may read or write resource of type `A`, may read resource of type `B`,
    ///     // and may prepare & execute queries of `(&mut B, &mut C)`.
    /// }
    ///
    /// let mut increment = 0;
    /// // All together, systems require resources of types `A`, `B`, and `C`.
    /// let mut executor = Executor::<(A, B, C)>::builder()
    ///     .system(system_0)
    ///     .system(system_1)
    ///     .system(|context, res_c: &C, _queries: &mut ()| {
    ///         // This system may read resource of type `C` and will not perform any queries.
    ///         increment += 1; // `increment` will be borrowed by the executor.
    ///     })
    ///     .build();
    /// let (mut a, mut b, mut c) = (A, B, C);
    /// executor.run(&world, (&mut a, &mut b, &mut c));
    /// executor.run(&world, (&mut a, &mut b, &mut c));
    /// executor.run(&world, (&mut a, &mut b, &mut c));
    /// drop(executor); // This releases the borrow of `increment`.
    /// assert_eq!(increment, 3);
    /// ```
    pub fn system<'a, Closure, ResourceRefs, Queries, Markers>(mut self, closure: Closure) -> Self
    where
        Resources::Wrapped: 'a,
        Closure: FnMut(SystemContext<'a>, ResourceRefs, &mut Queries) + Send + Sync + 'closures,
        ResourceRefs: Fetch<'a, Resources::Wrapped, Markers> + 'a,
        Queries: QueryBundle + Send + Sync + 'closures,
    {
        let id = SystemId(self.systems.len());
        let system = Self::box_system::<'a, Closure, ResourceRefs, Queries, Markers>(closure);
        #[cfg(feature = "parallel")]
        {
            self.all_component_types
                .extend(&system.component_type_set.immutable);
            self.all_component_types
                .extend(&system.component_type_set.mutable);
        }
        self.systems.insert(id, system);
        self
    }

    /// Like `system` but allows for a non `Send + Sync` closure and a second set of non `Sync`
    /// resources.
    pub fn local_system<
        'a,
        Closure,
        ResourceRefs,
        LocalResourceRefs,
        Queries,
        Markers,
        LocalMarkers,
    >(
        mut self,
        closure: Closure,
    ) -> Self
    where
        Resources::Wrapped: 'a,
        LocalResources::Wrapped: 'a,
        Closure:
            FnMut(SystemContext<'a>, ResourceRefs, LocalResourceRefs, &mut Queries) + 'closures,
        ResourceRefs: Fetch<'a, Resources::Wrapped, Markers> + 'a,
        LocalResourceRefs: Fetch<'a, LocalResources::Wrapped, LocalMarkers> + 'a,
        Queries: QueryBundle + 'closures,
    {
        let id = SystemId(self.systems.len());
        let system = Self::box_local_system::<
            'a,
            Closure,
            ResourceRefs,
            LocalResourceRefs,
            Queries,
            Markers,
            LocalMarkers,
        >(closure);
        #[cfg(feature = "parallel")]
        {
            self.all_component_types
                .extend(&system.component_type_set.immutable);
            self.all_component_types
                .extend(&system.component_type_set.mutable);
        }
        self.systems.insert(id, system);
        self
    }

    /// Creates a new system from a closure or a function, and inserts it into
    /// the builder with given handle; see [`::system()`](#method.system).
    ///
    /// Handles allow defining relative order of execution between systems;
    /// doing that is optional. They can be of any type that is `Sized + Eq + Hash + Debug`
    /// and do not persist after [`::build()`](struct.ExecutorBuilder.html#method.build) - the
    /// resulting executor relies on lightweight opaque IDs;
    /// see [`SystemContext::id()`](struct.SystemContext.html#method.id).
    ///
    /// Handles must be unique, and systems with dependencies must be inserted
    /// into the builder after said dependencies.
    /// If the default `parallel` feature is disabled the systems will be executed in insertion
    /// order, which these rules guarantee to be a valid order.
    ///
    /// Since specifying a dependency between systems forbids them to run concurrently, this
    /// functionality should be used only when necessary. In fact, for executors where systems
    /// form a single chain of execution it is more performant to call them as functions,
    /// in a sequence, inside a single [`rayon::scope()`](../rayon/fn.scope.html) or
    /// [`rayon::ThreadPool::install()`](../rayon/struct.ThreadPool.html#method.install) block.
    ///
    /// # Examples
    /// These two executors are identical.
    /// ```rust
    /// # use hv_yaks::{QueryMarker, SystemContext, Executor};
    /// # let world = hv_ecs::World::new();
    /// # fn system_0(_: SystemContext, _: (), _: &mut ()) {}
    /// # fn system_1(_: SystemContext, _: (), _: &mut ()) {}
    /// # fn system_2(_: SystemContext, _: (), _: &mut ()) {}
    /// # fn system_3(_: SystemContext, _: (), _: &mut ()) {}
    /// # fn system_4(_: SystemContext, _: (), _: &mut ()) {}
    /// let _ = Executor::<()>::builder()
    ///     .system_with_handle(system_0, 0)
    ///     .system_with_handle(system_1, 1)
    ///     .system_with_handle_and_deps(system_2, 2, vec![0, 1])
    ///     .system_with_deps(system_3, vec![2])
    ///     .system_with_deps(system_4, vec![0])
    ///     .build();
    /// let _ = Executor::<()>::builder()
    ///     .system_with_handle(system_0, "system_0")
    ///     .system_with_handle(system_1, "system_1")
    ///     .system_with_handle_and_deps(system_2, "system_2", vec!["system_1", "system_0"])
    ///     .system_with_deps(system_3, vec!["system_2"])
    ///     .system_with_deps(system_4, vec!["system_0"])
    ///     .build();
    /// ```
    /// The order of execution (with the default `parallel` feature enabled) is:
    /// - systems 0 ***and*** 1,
    /// - system 4 as soon as 0 is finished ***and*** system 2 as soon as both 0 and 1 are finished,
    /// - system 3 as soon as 2 is finished.
    ///
    /// Note that system 4 may start running before system 1 has finished, and,
    /// if it's calculations take long enough, might finish last, after system 3.
    ///
    /// This executor will behave identically to the two above if the default `parallel`
    /// feature is enabled; otherwise, the execution order will be different from theirs, but
    /// that won't matter as long as the given dependencies truthfully reflect any
    /// relationships the systems may have.
    /// ```rust
    /// # use hv_yaks::{QueryMarker, SystemContext, Executor};
    /// # let world = hv_ecs::World::new();
    /// # fn system_0(_: SystemContext, _: (), _: &mut ()) {}
    /// # fn system_1(_: SystemContext, _: (), _: &mut ()) {}
    /// # fn system_2(_: SystemContext, _: (), _: &mut ()) {}
    /// # fn system_3(_: SystemContext, _: (), _: &mut ()) {}
    /// # fn system_4(_: SystemContext, _: (), _: &mut ()) {}
    /// let _ = Executor::<()>::builder()
    ///     .system_with_handle(system_1, 1)
    ///     .system_with_handle(system_0, 0)
    ///     .system_with_deps(system_4, vec![0])
    ///     .system_with_handle_and_deps(system_2, 2, vec![0, 1])
    ///     .system_with_deps(system_3, vec![2])
    ///     .build();
    /// ```
    ///
    /// # Panics
    /// This function will panic if:
    /// - a system with given handle is already present in the builder.
    pub fn system_with_handle<'a, Closure, ResourceRefs, Queries, Markers, NewHandle>(
        mut self,
        closure: Closure,
        handle: NewHandle,
    ) -> ExecutorBuilder<'closures, Resources, LocalResources, NewHandle>
    where
        Resources::Wrapped: 'a,
        Closure: FnMut(SystemContext<'a>, ResourceRefs, &mut Queries) + Send + Sync + 'closures,
        ResourceRefs: Fetch<'a, Resources::Wrapped, Markers> + 'a,
        Queries: QueryBundle + Send + Sync + 'closures,
        NewHandle: HandleConversion<Handle> + Debug,
    {
        let mut handles = NewHandle::convert_hash_map(self.handles);
        assert!(
            !handles.contains_key(&handle),
            "system {:?} already exists",
            handle
        );
        let id = SystemId(self.systems.len());
        let system = Self::box_system::<'a, Closure, ResourceRefs, Queries, Markers>(closure);
        #[cfg(feature = "parallel")]
        {
            self.all_component_types
                .extend(&system.component_type_set.immutable);
            self.all_component_types
                .extend(&system.component_type_set.mutable);
        }
        self.systems.insert(id, system);
        handles.insert(handle, id);
        ExecutorBuilder {
            systems: self.systems,
            handles,
            #[cfg(feature = "parallel")]
            all_component_types: self.all_component_types,
        }
    }

    /// Thread-local version of [`system_with_handle`].
    pub fn local_system_with_handle<
        'a,
        Closure,
        ResourceRefs,
        LocalResourceRefs,
        Queries,
        Markers,
        LocalMarkers,
        NewHandle,
    >(
        mut self,
        closure: Closure,
        handle: NewHandle,
    ) -> ExecutorBuilder<'closures, Resources, LocalResources, NewHandle>
    where
        Resources::Wrapped: 'a,
        LocalResources::Wrapped: 'a,
        Closure:
            FnMut(SystemContext<'a>, ResourceRefs, LocalResourceRefs, &mut Queries) + 'closures,
        ResourceRefs: Fetch<'a, Resources::Wrapped, Markers> + 'a,
        LocalResourceRefs: Fetch<'a, LocalResources::Wrapped, LocalMarkers> + 'a,
        Queries: QueryBundle + 'closures,
        NewHandle: HandleConversion<Handle> + Debug,
    {
        let mut handles = NewHandle::convert_hash_map(self.handles);
        assert!(
            !handles.contains_key(&handle),
            "system {:?} already exists",
            handle
        );
        let id = SystemId(self.systems.len());
        let system = Self::box_local_system::<
            'a,
            Closure,
            ResourceRefs,
            LocalResourceRefs,
            Queries,
            Markers,
            LocalMarkers,
        >(closure);
        #[cfg(feature = "parallel")]
        {
            self.all_component_types
                .extend(&system.component_type_set.immutable);
            self.all_component_types
                .extend(&system.component_type_set.mutable);
        }
        self.systems.insert(id, system);
        handles.insert(handle, id);
        ExecutorBuilder {
            systems: self.systems,
            handles,
            #[cfg(feature = "parallel")]
            all_component_types: self.all_component_types,
        }
    }

    /// Creates a new system from a closure or a function, and inserts it into
    /// the builder with given dependencies; see [`::system()`](#method.system).
    ///
    /// Given system will start running only after all systems in given list of dependencies
    /// have finished running.
    ///
    /// This function cannot be used unless the builder already has
    /// at least one system with a handle;
    /// see [`::system_with_handle()`](#method.system_with_handle).
    ///
    /// # Panics
    /// This function will panic if:
    /// - given list of dependencies contains a handle that
    /// doesn't correspond to any system in the builder.
    pub fn system_with_deps<'a, Closure, ResourceRefs, Queries, Markers>(
        mut self,
        closure: Closure,
        dependencies: Vec<Handle>,
    ) -> Self
    where
        Resources::Wrapped: 'a,
        Closure: FnMut(SystemContext<'a>, ResourceRefs, &mut Queries) + Send + Sync + 'closures,
        ResourceRefs: Fetch<'a, Resources::Wrapped, Markers> + 'a,
        Queries: QueryBundle + Send + Sync + 'closures,
        Handle: Eq + Hash + Debug,
    {
        let id = SystemId(self.systems.len());
        let mut system = Self::box_system::<'a, Closure, ResourceRefs, Queries, Markers>(closure);
        #[cfg(feature = "parallel")]
        {
            self.all_component_types
                .extend(&system.component_type_set.immutable);
            self.all_component_types
                .extend(&system.component_type_set.mutable);
        }
        system
            .dependencies
            .extend(dependencies.iter().map(|dep_handle| {
                *self.handles.get(dep_handle).unwrap_or_else(|| {
                    panic!(
                    "could not resolve dependencies of a handle-less system: no system {:?} found",
                    dep_handle
                )
                })
            }));
        self.systems.insert(id, system);
        self
    }

    /// Thread-local version of [`system_with_deps`].
    pub fn local_system_with_deps<
        'a,
        Closure,
        ResourceRefs,
        LocalResourceRefs,
        Queries,
        Markers,
        LocalMarkers,
    >(
        mut self,
        closure: Closure,
        dependencies: Vec<Handle>,
    ) -> Self
    where
        Resources::Wrapped: 'a,
        LocalResources::Wrapped: 'a,
        Closure:
            FnMut(SystemContext<'a>, ResourceRefs, LocalResourceRefs, &mut Queries) + 'closures,
        ResourceRefs: Fetch<'a, Resources::Wrapped, Markers> + 'a,
        LocalResourceRefs: Fetch<'a, LocalResources::Wrapped, LocalMarkers> + 'a,
        Queries: QueryBundle + 'closures,
        Handle: Eq + Hash + Debug,
    {
        let id = SystemId(self.systems.len());
        let mut system = Self::box_local_system::<
            'a,
            Closure,
            ResourceRefs,
            LocalResourceRefs,
            Queries,
            Markers,
            LocalMarkers,
        >(closure);
        #[cfg(feature = "parallel")]
        {
            self.all_component_types
                .extend(&system.component_type_set.immutable);
            self.all_component_types
                .extend(&system.component_type_set.mutable);
        }
        system
            .dependencies
            .extend(dependencies.iter().map(|dep_handle| {
                *self.handles.get(dep_handle).unwrap_or_else(|| {
                    panic!(
                    "could not resolve dependencies of a handle-less system: no system {:?} found",
                    dep_handle
                )
                })
            }));
        self.systems.insert(id, system);
        self
    }

    /// Creates a new system from a closure or a function, and inserts it into
    /// the builder with given handle and dependencies; see [`::system()`](#method.system).
    ///
    /// Given system will start running only after all systems in given list of dependencies
    /// have finished running.
    ///
    /// This function cannot be used unless the builder already has
    /// at least one system with a handle;
    /// see [`::system_with_handle()`](#method.system_with_handle).
    ///
    /// # Panics
    /// This function will panic if:
    /// - a system with given handle is already present in the builder,
    /// - given list of dependencies contains a handle that
    /// doesn't correspond to any system in the builder,
    /// - given handle appears in given list of dependencies.
    pub fn system_with_handle_and_deps<'a, Closure, ResourceRefs, Queries, Markers>(
        mut self,
        closure: Closure,
        handle: Handle,
        dependencies: Vec<Handle>,
    ) -> Self
    where
        Resources::Wrapped: 'a,
        Closure: FnMut(SystemContext<'a>, ResourceRefs, &mut Queries) + Send + Sync + 'closures,
        ResourceRefs: Fetch<'a, Resources::Wrapped, Markers> + 'a,
        Queries: QueryBundle + Send + Sync + 'closures,
        Handle: Eq + Hash + Debug,
    {
        assert!(
            !self.handles.contains_key(&handle),
            "system {:?} already exists",
            handle
        );
        assert!(
            !dependencies.contains(&handle),
            "system {:?} depends on itself",
            handle
        );
        let id = SystemId(self.systems.len());
        let mut system = Self::box_system::<'a, Closure, ResourceRefs, Queries, Markers>(closure);
        #[cfg(feature = "parallel")]
        {
            self.all_component_types
                .extend(&system.component_type_set.immutable);
            self.all_component_types
                .extend(&system.component_type_set.mutable);
        }
        system
            .dependencies
            .extend(dependencies.iter().map(|dep_handle| {
                *self.handles.get(dep_handle).unwrap_or_else(|| {
                    panic!(
                        "could not resolve dependencies of system {:?}: no system {:?} found",
                        handle, dep_handle
                    )
                })
            }));
        self.systems.insert(id, system);
        self.handles.insert(handle, id);
        self
    }

    /// Thread-local version of [`system_with_handle_and_deps`].
    pub fn local_system_with_handle_and_deps<
        'a,
        Closure,
        ResourceRefs,
        LocalResourceRefs,
        Queries,
        Markers,
        LocalMarkers,
    >(
        mut self,
        closure: Closure,
        handle: Handle,
        dependencies: Vec<Handle>,
    ) -> Self
    where
        Resources::Wrapped: 'a,
        LocalResources::Wrapped: 'a,
        Closure:
            FnMut(SystemContext<'a>, ResourceRefs, LocalResourceRefs, &mut Queries) + 'closures,
        ResourceRefs: Fetch<'a, Resources::Wrapped, Markers> + 'a,
        LocalResourceRefs: Fetch<'a, LocalResources::Wrapped, LocalMarkers> + 'a,
        Queries: QueryBundle + 'closures,
        Handle: Eq + Hash + Debug,
    {
        assert!(
            !self.handles.contains_key(&handle),
            "system {:?} already exists",
            handle
        );
        assert!(
            !dependencies.contains(&handle),
            "system {:?} depends on itself",
            handle
        );
        let id = SystemId(self.systems.len());
        let mut system = Self::box_local_system::<
            'a,
            Closure,
            ResourceRefs,
            LocalResourceRefs,
            Queries,
            Markers,
            LocalMarkers,
        >(closure);
        #[cfg(feature = "parallel")]
        {
            self.all_component_types
                .extend(&system.component_type_set.immutable);
            self.all_component_types
                .extend(&system.component_type_set.mutable);
        }
        system
            .dependencies
            .extend(dependencies.iter().map(|dep_handle| {
                *self.handles.get(dep_handle).unwrap_or_else(|| {
                    panic!(
                        "could not resolve dependencies of system {:?}: no system {:?} found",
                        handle, dep_handle
                    )
                })
            }));
        self.systems.insert(id, system);
        self.handles.insert(handle, id);
        self
    }

    /// Consumes the builder and returns the finalized executor.
    pub fn build(self) -> Executor<'closures, Resources, LocalResources> {
        Executor::build(self)
    }
}

#[derive(PartialEq, Eq, Hash)]
pub struct DummyHandle;

pub trait HandleConversion<T>: Sized + Eq + Hash {
    fn convert_hash_map(map: HashMap<T, SystemId>) -> HashMap<Self, SystemId>;
}

impl<T> HandleConversion<DummyHandle> for T
where
    T: Debug + Eq + Hash,
{
    fn convert_hash_map(_: HashMap<DummyHandle, SystemId>) -> HashMap<Self, SystemId> {
        HashMap::new()
    }
}

impl<T> HandleConversion<T> for T
where
    T: Debug + Eq + Hash,
{
    fn convert_hash_map(map: HashMap<T, SystemId>) -> HashMap<Self, SystemId> {
        map
    }
}
