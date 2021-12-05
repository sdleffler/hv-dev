use hv_ecs::{
    Archetype, ArchetypesGeneration, Column, ColumnMut, Component, Entity, NoSuchEntity,
    PreparedQuery, PreparedQueryBorrow, Query, QueryBorrow, QueryOne, World,
};

use crate::{QueryMarker, SystemId};

/// Thin wrapper over [`hv_ecs::World`](../hv-ecs/struct.World.html), can prepare queries using a
/// [`QueryMarker`](struct.QueryMarker.html).
///
/// It cannot be instantiated directly. See [`System`](trait.System.html) for instructions
/// on how to call systems outside of an executor, as plain functions.
pub struct SystemContext<'scope> {
    pub(crate) system_id: Option<SystemId>,
    pub(crate) world: &'scope World,
}

impl<'scope> SystemContext<'scope> {
    /// Returns a debug-printable `SystemId` if the system is ran in an
    /// [`Executor`](struct.Executor.html), with printed number reflecting
    /// the order of insertion into the [`ExecutorBuilder`](struct.ExecutorBuilder.html).
    pub fn id(&self) -> Option<SystemId> {
        self.system_id
    }

    /// Prepares a query using the given [`QueryMarker`](struct.QueryMarker.html);
    /// see [`hv_ecs::World::query()`](../hv-ecs/struct.World.html#method.query).
    ///
    /// # Example
    /// ```rust
    /// # use hv_yaks::{SystemContext, QueryMarker};
    /// # struct Pos;
    /// # #[derive(Clone, Copy)]
    /// # struct Vel;
    /// # impl std::ops::AddAssign<Vel> for Pos {
    /// #     fn add_assign(&mut self, _: Vel) {}
    /// # }
    /// # let world = hv_ecs::World::new();
    /// fn some_system(
    ///     context: SystemContext,
    ///     _resources: (),
    ///     &mut query: &mut QueryMarker<(&mut Pos, &Vel)>
    /// ) {
    ///     for (_entity, (pos, vel)) in context.query(query).iter() {
    ///         *pos += *vel;
    ///     }
    /// };
    /// ```
    pub fn query<Q>(&self, _: QueryMarker<Q>) -> QueryBorrow<'_, Q>
    where
        Q: Query + Send + Sync,
    {
        self.world.query()
    }

    /// Perform a query with a cache of the query results. See [`PreparedQuery`] for more.
    ///
    /// # Example
    /// ```rust
    /// # use hv_yaks::{SystemContext};
    /// # use hv_ecs::PreparedQuery;
    /// # struct Pos;
    /// # #[derive(Clone, Copy)]
    /// # struct Vel;
    /// # impl std::ops::AddAssign<Vel> for Pos {
    /// #     fn add_assign(&mut self, _: Vel) {}
    /// # }
    /// # let world = hv_ecs::World::new();
    /// fn some_system(
    ///     context: SystemContext,
    ///     _resources: (),
    ///     query: &mut PreparedQuery<(&mut Pos, &Vel)>
    /// ) {
    ///     for (_entity, (pos, vel)) in context.prepared_query(query).iter() {
    ///         *pos += *vel;
    ///     }
    /// };
    /// ```
    pub fn prepared_query<'q, Q>(
        &'q self,
        prepared_query: &'q mut PreparedQuery<Q>,
    ) -> PreparedQueryBorrow<'q, Q>
    where
        Q: Query + Send + Sync,
    {
        prepared_query.query(self.world)
    }

    /// Prepares a query against a single entity using the given
    /// [`QueryMarker`](struct.QueryMarker.html);
    /// see [`hv_ecs::World::query_one()`](../hv-ecs/struct.World.html#method.query_one).
    ///
    /// # Example
    /// ```rust
    /// # use hv_yaks::{SystemContext, QueryMarker};
    /// # #[derive(Default)]
    /// # struct Pos;
    /// # #[derive(Clone, Copy, Default, Ord, PartialOrd, Eq, PartialEq)]
    /// # struct Vel;
    /// # impl std::ops::AddAssign<Vel> for Pos {
    /// #     fn add_assign(&mut self, _: Vel) {}
    /// # }
    /// # let world = hv_ecs::World::new();
    /// fn some_system(
    ///     context: SystemContext,
    ///     _resources: (),
    ///     query: QueryMarker<(&mut Pos, &Vel)>
    /// ) {
    ///     let mut max_velocity = Vel::default();
    ///     let mut max_velocity_entity = None;
    ///     for (entity, (pos, vel)) in context.query(query).iter() {
    ///         *pos += *vel;
    ///         if *vel > max_velocity {
    ///             max_velocity = *vel;
    ///             max_velocity_entity = Some(entity);
    ///         }
    ///     }
    ///     if let Some(entity) = max_velocity_entity {
    ///         let mut query_one = context
    ///             .query_one(query, entity)
    ///             .expect("no such entity");
    ///         let (pos, _vel) = query_one
    ///             .get()
    ///             .expect("some components are missing");
    ///         *pos = Pos::default();
    ///     }
    /// };
    /// ```
    pub fn query_one<Q>(
        &self,
        _: QueryMarker<Q>,
        entity: Entity,
    ) -> Result<QueryOne<'_, Q>, NoSuchEntity>
    where
        Q: Query + Send + Sync,
    {
        self.world.query_one(entity)
    }

    /// Immutably borrow every `T` component in the world for efficient random access.
    ///
    /// Panics if this would conflict with an outstanding borrow.
    pub fn column<T>(&self, _: QueryMarker<&T>) -> Column<'_, T>
    where
        T: Component,
    {
        self.world.column()
    }

    /// Mutably borrow every `T` component in the world for efficient random access.
    ///
    /// Panics if this would conflict with an outstanding borrow.
    pub fn column_mut<T>(&self, _: QueryMarker<&mut T>) -> ColumnMut<'_, T>
    where
        T: Component,
    {
        self.world.column_mut()
    }

    /// See [`hv_ecs::World::reserve_entity()`](../hv-ecs/struct.World.html#method.reserve_entity).
    pub fn reserve_entity(&self) -> Entity {
        self.world.reserve_entity()
    }

    /// See [`hv_ecs::World::contains()`](../hv-ecs/struct.World.html#method.contains).
    pub fn contains(&self, entity: Entity) -> bool {
        self.world.contains(entity)
    }

    /// See [`hv_ecs::World::find_entity_from_id`]
    ///
    /// # Safety
    ///
    /// The id must have been previously obtained from an entity in this world which is still live.
    pub unsafe fn find_entity_from_id(&self, id: u32) -> Entity {
        self.world.find_entity_from_id(id)
    }

    /// See [`hv_ecs::World::archetypes()`](../hv-ecs/struct.World.html#method.archetypes).
    pub fn archetypes(&self) -> impl ExactSizeIterator<Item = &Archetype> + '_ {
        self.world.archetypes()
    }

    /// See [`hv_ecs::World::archetypes_generation()`][ag].
    ///
    /// [ag]: ../hv-ecs/struct.World.html#method.archetypes_generation
    pub fn archetypes_generation(&self) -> ArchetypesGeneration {
        self.world.archetypes_generation()
    }
}
