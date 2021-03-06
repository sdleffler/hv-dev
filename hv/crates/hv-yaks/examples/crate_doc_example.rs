//! Copy of the crate level documentation & readme example.

use hv_ecs::{With, Without, World};
use hv_yaks::{Executor, QueryMarker};

fn main() {
    let mut world = World::new();
    let mut entities = 0u32;
    world.spawn_batch((0..100u32).map(|index| {
        entities += 1;
        (index,)
    }));
    world.spawn_batch((0..100u32).map(|index| {
        entities += 1;
        (index, index as f32)
    }));
    let mut increment = 5usize;
    let mut average = 0f32;
    let mut executor = Executor::<(u32, usize, f32)>::builder()
        .system_with_handle(
            |context, (entities, average): (&u32, &mut f32), &mut query: &mut QueryMarker<&f32>| {
                *average = 0.0;
                for (_entity, float) in context.query(query).iter() {
                    *average += *float;
                }
                *average /= *entities as f32;
            },
            "average",
        )
        .system_with_handle(
            |context, increment: &usize, &mut query: &mut QueryMarker<&mut u32>| {
                for (_entity, unsigned) in context.query(query).iter() {
                    *unsigned += *increment as u32
                }
            },
            "increment",
        )
        .system_with_deps(system_with_two_queries, vec!["increment", "average"])
        .build();
    executor.run(&world, (&mut entities, &mut increment, &mut average));
}

#[allow(clippy::type_complexity)]
fn system_with_two_queries(
    context: hv_yaks::SystemContext,
    (entities, average): (&u32, &f32),
    &mut (with_f32, without_f32): &mut (
        QueryMarker<With<f32, &mut u32>>,
        QueryMarker<Without<f32, &mut u32>>,
    ),
) {
    hv_yaks::batch(
        &mut context.query(with_f32),
        entities / 8,
        |_entity, unsigned| {
            *unsigned += average.round() as u32;
        },
    );
    hv_yaks::batch(
        &mut context.query(without_f32),
        entities / 8,
        |_entity, unsigned| {
            *unsigned *= average.round() as u32;
        },
    );
}
