use hecs::World;
use hv_resources::Resources;

use crate::Executor;

use super::{ResourceTuple, ResourceWrap};

// TODO consider exposing.

pub trait MultiRefExtractor<Resources, LocalResources>
where
    Resources: ResourceTuple,
    Resources::Wrapped: Sync,
    Resources::BorrowTuple: Sync,
    LocalResources: ResourceTuple,
{
    fn extract_and_drive_executor(
        self,
        executor: &mut Executor<Resources, LocalResources>,
        world: &World,
    );
}

impl<T, U, Resources, LocalResources> MultiRefExtractor<Resources, LocalResources> for (T, U)
where
    Resources: ResourceTuple + RefExtractor<T>,
    Resources::Wrapped: Sync,
    Resources::BorrowTuple: Sync,
    LocalResources: ResourceTuple + RefExtractor<U>,
{
    fn extract_and_drive_executor(
        self,
        executor: &mut Executor<Resources, LocalResources>,
        world: &World,
    ) {
        let (resources, local_resources) = self;

        let Executor {
            borrows,
            local_borrows,
            inner,
        } = executor;

        Resources::extract_and_run(borrows, resources, |wrapped| {
            LocalResources::extract_and_run(local_borrows, local_resources, |local_wrapped| {
                inner.run(world, wrapped, local_wrapped);
            })
        });
    }
}

impl<'a, Rs, LocalRs> MultiRefExtractor<Rs, LocalRs> for &'a Resources
where
    Rs: ResourceTuple + RefExtractor<&'a Resources>,
    Rs::Wrapped: Sync,
    Rs::BorrowTuple: Sync,
    LocalRs: ResourceTuple + RefExtractor<&'a Resources>,
{
    fn extract_and_drive_executor(self, executor: &mut Executor<Rs, LocalRs>, world: &World) {
        let Executor {
            borrows,
            local_borrows,
            inner,
        } = executor;
        Rs::extract_and_run(borrows, self, |wrapped| {
            LocalRs::extract_and_run(local_borrows, self, |local_wrapped| {
                inner.run(world, wrapped, local_wrapped)
            })
        });
    }
}

/// Specifies how a tuple of references may be extracted from the implementor and used
/// as resources when running an executor.
pub trait RefExtractor<RefSource>: ResourceTuple + Sized {
    fn extract_and_run(
        borrows: &mut Self::BorrowTuple,
        resources: RefSource,
        f: impl FnOnce(Self::Wrapped),
    );
}

impl RefExtractor<()> for () {
    fn extract_and_run(
        _borrows: &mut Self::BorrowTuple,
        _resources: (),
        f: impl FnOnce(Self::Wrapped),
    ) {
        f(());
    }
}

impl<R0> RefExtractor<&mut R0> for (R0,)
where
    R0: Send,
{
    // fn extract_and_run(executor: &mut Executor<Self>, world: &World, mut resources: &mut R0) {
    //     let wrapped = resources.wrap(&mut executor.borrows);
    //     executor.inner.run(world, wrapped);
    // }

    fn extract_and_run(
        borrows: &mut Self::BorrowTuple,
        mut resources: &mut R0,
        f: impl FnOnce(Self::Wrapped),
    ) {
        f(resources.wrap(borrows));
    }
}

impl<R0> RefExtractor<(&mut R0,)> for (R0,)
where
    R0: Send,
{
    // fn extract_and_run(executor: &mut Executor<Self>, world: &World, mut resources: (&mut R0,)) {
    //     let wrapped = resources.wrap(&mut executor.borrows);
    //     executor.inner.run(world, wrapped);
    // }

    fn extract_and_run(
        borrows: &mut Self::BorrowTuple,
        mut resources: (&mut R0,),
        f: impl FnOnce(Self::Wrapped),
    ) {
        let wrapped = resources.wrap(borrows);
        f(wrapped);
    }
}

macro_rules! impl_ref_extractor {
    ($($letter:ident),*) => {
        impl<'a, $($letter),*> RefExtractor<($(&mut $letter,)*)> for ($($letter,)*)
        where
            $($letter: Send,)*
        {
            // fn extract_and_run(
            //     executor: &mut Executor<Self>,
            //     world: &World,
            //     mut resources: ($(&mut $letter,)*),
            // ) {
            //     let wrapped = resources.wrap(&mut executor.borrows);
            //     executor.inner.run(world, wrapped);
            // }

            fn extract_and_run(
                borrows: &mut Self::BorrowTuple,
                mut resources: ($(&mut $letter,)*),
                f: impl FnOnce(Self::Wrapped),
            ) {
                let wrapped = resources.wrap(borrows);
                f(wrapped);
            }
        }
    }
}

impl_for_tuples!(impl_ref_extractor);
