use std::fmt;
use std::{collections::HashMap, sync::Arc};

use hv::prelude::*;
use parry3d::{
    bounding_volume::BoundingVolume,
    query::{
        visitors::BoundingVolumeIntersectionsVisitor, Contact, DefaultQueryDispatcher,
        QueryDispatcher,
    },
    shape::{Shape, SimdCompositeShape, TriMesh},
    utils::IsometryOpt,
};
use slab::Slab;
use soft_edge::{CompoundHull, Exact, HullFacet};

#[inline]
fn hull_to_trimesh(hull: &CompoundHull) -> TriMesh {
    let mut index_map: HashMap<Exact, u32> = HashMap::new();
    let mut vertices: Vec<Point3<f32>> = Vec::new();
    let mut indices: Vec<[u32; 3]> = Vec::new();

    let mut v = |exact: Exact| -> u32 {
        *index_map.entry(exact).or_insert_with(|| {
            let i = vertices.len() as u32;
            vertices.push(exact.to_f32());
            i
        })
    };

    // Our winding order matches Parry's when it comes to normal calculation.
    for facet in hull.facets() {
        match facet {
            HullFacet::Triangle([a, b, c]) => indices.push([v(a), v(b), v(c)]),
            HullFacet::Rectangle([a, b, c, d]) => {
                indices.push([v(a), v(b), v(c)]);
                indices.push([v(b), v(c), v(d)]);
            }
        }
    }

    TriMesh::new(vertices, indices)
}

#[derive(Default, Clone)]
pub struct CompoundHullShapeCache {
    hulls: HashMap<CompoundHull, usize>,
    shapes: Slab<CompoundHullShape>,
}

impl fmt::Debug for CompoundHullShapeCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CompoundHullShapeCache")
            .field("shapes", &self.shapes.len())
            .finish()
    }
}

impl CompoundHullShapeCache {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn get_shape(&mut self, hull: &CompoundHull) -> CompoundHullShape {
        let &mut i = self
            .hulls
            .entry(hull.clone())
            .or_insert_with(|| self.shapes.insert(CompoundHullShape::new(hull)));
        self.shapes[i].clone()
    }
}

#[derive(Clone)]
pub struct CompoundHullShape {
    mesh: Arc<TriMesh>,
}

impl fmt::Debug for CompoundHullShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CompoundHullShape").finish_non_exhaustive()
    }
}

impl CompoundHullShape {
    pub fn new(hull: &CompoundHull) -> Self {
        Self {
            mesh: Arc::new(hull_to_trimesh(hull)),
        }
    }

    /// Perform a contact check by doing a roundabout SAT-like construction where we check against
    /// each triangle in the mesh, and in the end, we end up with either no contact or the contact
    /// with the least penetration. The "normal" implementation in Parry of a composite shape
    /// looking for a contact against a generic shape will return the contact with the *most*
    /// penetration; this is the worst possible case for us, and we want the least separating
    /// contact so that we can efficiently separate the two if they're colliding.
    ///
    /// This only works because we guarantee that the compound hull shape is approximately convex,
    /// in that it's a convex hull with holes in it (maybe) and we don't care about interior
    /// collisions (we just want to forget about it if any collisions happen to hit the holes.)
    pub fn contact(
        &self,
        pos1: &Isometry3<f32>,
        s2: &dyn Shape,
        pos2: &Isometry3<f32>,
        prediction: f32,
    ) -> Option<Contact> {
        // Use the local space of pos1.
        let pos12 = pos1.inv_mul(pos2);

        // Find new collisions
        let ls_aabb2 = s2.compute_aabb(&pos12).loosened(prediction);
        let mut res = None::<Contact>;

        let mut leaf_callback = |i: &_| {
            self.mesh.map_part_at(*i, &mut |part_pos1, part1| {
                if let Ok(Some(mut c)) = DefaultQueryDispatcher.contact(
                    &part_pos1.inv_mul(&pos12),
                    part1,
                    s2,
                    prediction,
                ) {
                    let is_front_face = self.mesh.triangle(*i).scaled_normal().dot(&c.normal1) > 0.;
                    let replace = res.map_or(true, |cbest| {
                        is_front_face && c.dist < 0. && c.dist > cbest.dist
                    });

                    if replace {
                        if let Some(part_pos1) = part_pos1 {
                            c.transform1_by_mut(part_pos1);
                        }
                        res = Some(c)
                    }
                }
            });

            true
        };

        let mut visitor = BoundingVolumeIntersectionsVisitor::new(&ls_aabb2, &mut leaf_callback);
        self.mesh.qbvh().traverse_depth_first(&mut visitor);
        res
    }
}
