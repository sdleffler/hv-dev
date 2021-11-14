use hv::{
    ecs::{Entity, QueryMarker, SystemContext},
    prelude::*,
};
use parry3d::{bounding_volume::AABB, partitioning::QBVH};
use shrev::ReaderId;
use std::collections::HashMap;

use crate::{
    lattice::{event::LatticeEvent, tracked_map::TrackedMap},
    physics::Collider,
};

// Pack three i32 coordinates into 3 bytes, 3 bytes, and 2 bytes.
fn pack_coords(coords: Vector3<i32>) -> u64 {
    assert!(coords.x.abs() < (1 << 23) - 1);
    assert!(coords.y.abs() < (1 << 23) - 1);
    assert!(coords.z.abs() < (1 << 15) - 1);

    let x = coords.x.to_le_bytes();
    let y = coords.y.to_le_bytes();
    let z = coords.z.to_le_bytes();
    u64::from_le_bytes([x[0], x[1], x[2], y[0], y[1], y[2], z[0], z[1]])
}

// Unpack coordinates created with `pack_coords`.
fn unpack_coords(packed: u64) -> Vector3<i32> {
    let b = packed.to_le_bytes();
    // retrieve the sign from the most significant bit.
    let mut xb = ((b[2] as i8).signum() as i32).to_le_bytes();
    xb[0..3].copy_from_slice(&b[0..3]);
    let mut yb = ((b[5] as i8).signum() as i32).to_le_bytes();
    yb[0..3].copy_from_slice(&b[3..6]);
    let mut zb = ((b[7] as i8).signum() as i32).to_le_bytes();
    zb[0..2].copy_from_slice(&b[6..8]);
    Vector3::new(
        i32::from_le_bytes(xb),
        i32::from_le_bytes(yb),
        i32::from_le_bytes(zb),
    )
}

#[derive(Debug, Default, Clone)]
pub struct Intersections {
    buf: Vec<u64>,
}

impl Intersections {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn drain(&mut self) -> impl Iterator<Item = Vector3<i32>> + '_ {
        self.buf.drain(..).map(unpack_coords)
    }
}

pub struct ColliderMap {
    entities_to_aabbs: HashMap<Entity, AABB>,

    // QBVH ids are encoded coordinates: x/y/z i32s cut down to 24 bit/24 bit/16 bit, using
    // pack_coords and unpack_coords.
    qbvh: QBVH<u64>,
    buf: Vec<(u64, AABB)>,

    reader_id: ReaderId<LatticeEvent<Entity>>,
}

impl ColliderMap {
    pub fn clear_cache(&mut self) {
        self.entities_to_aabbs.clear();
    }

    pub fn update(
        &mut self,
        map: &TrackedMap<Entity>,
        context: SystemContext,
        query: QueryMarker<&Collider>,
    ) {
        // for &event in map.events().read(&mut self.reader_id) {
        //     match event {
        //         // if there's a slot event, then *only if* it's an insert w/ a previous value, we
        //         // can get by without rebuilding.
        //         LatticeEvent::Slot(SlotEvent {
        //             kind: SlotEventKind::Insert { prev: Some(_), .. },
        //             ..
        //         }) => {}
        //         // if it's a slot event that inserts "from zero" or  there's a whole-chunk or whole-layer event, we need to rebuild since things
        //         // may be removed or added.
        //         _ => self.needs_rebuild = true,
        //     }

        //     self.debouncer.push(event);
        // }

        if map.events().read(&mut self.reader_id).len() > 0 {
            // FIXME(sleffy): report warnings when unable to extract a collider/AABB from an entity
            let qbvh_generator = map.as_chunk_map().iter().filter_map(|(coords, &entity)| {
                use std::collections::hash_map::Entry::*;

                let aabb = match self.entities_to_aabbs.entry(entity) {
                    Occupied(occupied) => *occupied.get(),
                    Vacant(vacant) => {
                        let res = context
                            .query_one(query, entity)
                            .ok()?
                            .get()?
                            .compute_aabb(&Isometry3::from(coords.cast::<f32>()));
                        vacant.insert(res);
                        res
                    }
                };

                Some((pack_coords(coords), aabb))
            });

            self.buf.clear();
            self.buf.extend(qbvh_generator);
            self.qbvh.clear_and_rebuild(self.buf.drain(..), 0.05);
        }
    }

    pub fn intersect(&self, aabb: &AABB, out: &mut Intersections) {
        self.qbvh.intersect_aabb(aabb, &mut out.buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_ok() {
        fn roundtrip(v: Vector3<i32>) {
            assert_eq!(unpack_coords(pack_coords(v)), v);
        }

        roundtrip(Vector3::new(-23, 0, 4));
        roundtrip(Vector3::new(0, 0, 0));
        roundtrip(Vector3::new(-1, -1, -1));
        roundtrip(Vector3::new((1 >> 23) - 1, -(1 >> 23), (1 >> 15) - 1));
        roundtrip(Vector3::new(-(1 >> 23), (1 >> 23) - 1, -(1 >> 15)));
    }

    #[test]
    #[should_panic]
    fn pack_unpack_fail_x() {
        pack_coords(Vector3::new(i32::MAX, i32::MIN, i16::MAX as i32 + 1));
    }

    #[test]
    #[should_panic]
    fn pack_unpack_fail_z() {
        pack_coords(Vector3::new(0, 0, i16::MIN as i32 - 1));
    }
}
