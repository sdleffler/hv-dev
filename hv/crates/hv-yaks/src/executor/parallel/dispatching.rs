use hv_ecs::World;
use parking_lot::Mutex;
use rayon::prelude::*;
use std::{collections::HashMap, sync::Arc};

use super::SystemClosure;
use crate::{resource::ResourceTuple, SystemContext, SystemId};

/// Parallel executor variant, used when all systems are proven to be statically disjoint,
/// and have no dependencies.
pub struct Dispatcher<'closures, Resources>
where
    Resources: ResourceTuple,
    Resources::Wrapped: Sync,
    Resources::BorrowTuple: Sync,
{
    pub systems: HashMap<SystemId, Arc<Mutex<SystemClosure<'closures, Resources::Wrapped>>>>,
}

impl<'closures, Resources> Dispatcher<'closures, Resources>
where
    Resources: ResourceTuple,
    Resources::Wrapped: Sync,
    Resources::BorrowTuple: Sync,
{
    pub fn run(&mut self, world: &World, wrapped: Resources::Wrapped) {
        // All systems are statically disjoint, so they can all be running together at all times.
        self.systems.par_iter().for_each(|(id, system)| {
            let system = &mut *system
                .try_lock() // TODO should this be .lock() instead?
                .expect("systems should only be ran once per execution");
            system(
                SystemContext {
                    system_id: Some(*id),
                    world,
                },
                &wrapped,
            );
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::ExecutorParallel;
    use crate::{
        resource::{AtomicBorrow, ResourceWrap},
        Executor, QueryMarker,
    };
    use hv_ecs::World;

    struct A(usize);
    struct B(usize);
    struct C(usize);

    #[test]
    fn trivial() {
        ExecutorParallel::<(), ()>::build(
            Executor::builder()
                .system(|_, _: (), _: &mut ()| {})
                .system(|_, _: (), _: &mut ()| {}),
        )
        .unwrap_to_dispatcher();
    }

    #[test]
    fn trivial_with_resources() {
        ExecutorParallel::<(A, B, C), ()>::build(
            Executor::builder()
                .system(|_, _: (), _: &mut ()| {})
                .system(|_, _: (), _: &mut ()| {}),
        )
        .unwrap_to_dispatcher();
    }

    #[test]
    fn resources_disjoint() {
        let world = World::new();
        let mut a = A(0);
        let mut b = B(1);
        let mut c = C(2);
        let mut executor = ExecutorParallel::<(A, B, C), ()>::build(
            Executor::builder()
                .system(|_, (a, c): (&mut A, &C), _: &mut ()| {
                    a.0 += c.0;
                })
                .system(|_, (b, c): (&mut B, &C), _: &mut ()| {
                    b.0 += c.0;
                }),
        )
        .unwrap_to_dispatcher();
        let mut borrows = (
            AtomicBorrow::new(),
            AtomicBorrow::new(),
            AtomicBorrow::new(),
        );
        let wrapped = (&mut a, &mut b, &mut c).wrap(&mut borrows);
        executor.run(&world, wrapped);
        assert_eq!(a.0, 2);
        assert_eq!(b.0, 3);
    }

    #[test]
    fn components_disjoint() {
        let mut world = World::new();
        world.spawn_batch((0..10).map(|_| (A(0), B(0), C(0))));
        let mut a = A(1);
        let mut executor = ExecutorParallel::<(A,), ()>::build(
            Executor::builder()
                .system(|ctx, a: &A, q: &mut QueryMarker<(&A, &mut B)>| {
                    for (_, (_, b)) in ctx.query(*q).iter() {
                        b.0 += a.0;
                    }
                })
                .system(|ctx, a: &A, q: &mut QueryMarker<(&A, &mut C)>| {
                    for (_, (_, c)) in ctx.query(*q).iter() {
                        c.0 += a.0;
                    }
                }),
        )
        .unwrap_to_dispatcher();
        let mut borrow = (AtomicBorrow::new(),);
        let wrapped = (&mut a).wrap(&mut borrow);
        executor.run(&world, wrapped);
        for (_, (b, c)) in world.query::<(&B, &C)>().iter() {
            assert_eq!(b.0, 1);
            assert_eq!(c.0, 1);
        }
    }
}
