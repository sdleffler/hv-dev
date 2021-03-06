use hv_ecs::World;

use crate::{QueryBundle, SystemContext};

// TODO improve doc
/// Automatically implemented on all closures and functions than
/// can be used as systems in an executor.
pub trait System<'closure, Resources, Queries, RefSource, Marker> {
    /// Zero-cost wrapping function that executes the system.
    fn run(&mut self, world: &World, resources: RefSource);
}

impl<'closure, Closure, Resources, Queries> System<'closure, Resources, Queries, Resources, ()>
    for Closure
where
    Closure: FnMut(SystemContext, Resources, &mut Queries) + 'closure,
    Queries: QueryBundle + 'closure,
{
    fn run(&mut self, world: &World, resources: Resources) {
        self(
            SystemContext {
                system_id: None,
                world,
            },
            resources,
            &mut Queries::markers(),
        );
    }
}

#[test]
fn smoke_test() {
    let world = hv_ecs::World::new();

    fn dummy_system(_: SystemContext, _: (), _: &mut ()) {}
    dummy_system.run(&world, ());

    let mut counter = 0i32;
    fn increment_system(_: SystemContext, value: &mut i32, _: &mut ()) {
        *value += 1;
    }
    increment_system.run(&world, &mut counter);
    assert_eq!(counter, 1);

    let increment = 3usize;
    fn sum_system(_: SystemContext, (a, b): (&mut i32, &usize), _: &mut ()) {
        *a += *b as i32;
    }
    sum_system.run(&world, (&mut counter, &increment));
    assert_eq!(counter, 4);
    sum_system.run(&world, (&mut counter, &increment));
    assert_eq!(counter, 7);
}
