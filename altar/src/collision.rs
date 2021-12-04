use std::fmt;
use std::{collections::HashMap, sync::Arc};

use hv::prelude::*;
use parry3d::{
    bounding_volume::BoundingVolume,
    query::{
        visitors::BoundingVolumeIntersectionsVisitor, Contact, DefaultQueryDispatcher, PointQuery,
        PointQueryWithLocation, QueryDispatcher,
    },
    shape::{FeatureId, Shape, SimdCompositeShape, TriMesh, TrianglePointLocation},
    utils::IsometryOpt,
};
use slab::Slab;
use soft_edge::{CompoundHull, EdgeFilter, Exact, HullFacet, SortedPair, VertexFilter};

#[derive(Default, Clone)]
pub struct CompoundHullShapeCache {
    hulls: HashMap<CompoundHull, usize>,
    shapes: Slab<Arc<CompoundHullShape>>,
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
    pub fn get_shape(&mut self, hull: &CompoundHull) -> Arc<CompoundHullShape> {
        let &mut i = self
            .hulls
            .entry(hull.clone())
            .or_insert_with(|| self.shapes.insert(Arc::new(CompoundHullShape::new(hull))));
        self.shapes[i].clone()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Hash)]
pub enum CompoundHullFeature {
    Edge(SortedPair<Exact>),
    Vertex(Exact),
}

#[derive(Clone)]
pub struct CompoundHullShape {
    mesh: TriMesh,
    features: HashMap<(u32, FeatureId), CompoundHullFeature>,
}

impl fmt::Debug for CompoundHullShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CompoundHullShape").finish_non_exhaustive()
    }
}

impl CompoundHullShape {
    pub fn new(hull: &CompoundHull) -> Self {
        let mut index_map: HashMap<Exact, u32> = HashMap::new();
        let mut vertices: Vec<Point3<f32>> = Vec::new();
        let mut indices: Vec<[u32; 3]> = Vec::new();
        let mut features = HashMap::new();

        let mut v = |exact: Exact| -> u32 {
            *index_map.entry(exact).or_insert_with(|| {
                let i = vertices.len() as u32;
                vertices.push(exact.to_f32());
                i
            })
        };

        let mut feature = |subshape_id: u32, a: Exact, b: Exact, c: Exact| {
            for (i, v) in [a, b, c].into_iter().enumerate() {
                features.insert(
                    (subshape_id, FeatureId::Vertex(i as u32)),
                    CompoundHullFeature::Vertex(v),
                );
            }

            for (i, e) in [(a, b), (b, c), (a, c)].into_iter().enumerate() {
                features.insert(
                    (subshape_id, FeatureId::Edge(i as u32)),
                    CompoundHullFeature::Edge(SortedPair::new(e.0, e.1)),
                );
            }
        };

        // Our winding order matches Parry's when it comes to normal calculation.
        for facet in hull.facets() {
            match facet {
                HullFacet::Triangle([a, b, c]) => {
                    feature(indices.len() as u32, a, b, c);
                    indices.push([v(a), v(b), v(c)]);
                }
                HullFacet::Rectangle([a, b, c, d]) => {
                    feature(indices.len() as u32, a, b, c);
                    indices.push([v(a), v(b), v(c)]);
                    feature(indices.len() as u32, a, c, d);
                    indices.push([v(a), v(c), v(d)]);
                }
            }
        }

        let mesh = TriMesh::new(vertices, indices);

        Self { mesh, features }
    }

    /// Perform a contact check by doing a roundabout SAT-like construction.
    ///
    /// We check against each triangle in the mesh, and in the end, we end up with either no contact
    /// or the contact with the least penetration. The "normal" implementation in Parry of a
    /// composite shape looking for a contact against a generic shape will return the contact with
    /// the *most* penetration; this is the worst possible case for us, and we want the least
    /// separating contact so that we can efficiently separate the two if they're colliding.
    ///
    /// This only works because we guarantee that the compound hull shape is approximately convex,
    /// in that it's a convex hull with holes in it (maybe) and we don't care about interior
    /// collisions (we just want to forget about it if any collisions happen to hit the holes.)
    pub fn contact(
        &self,
        &coords: &Vector3<i32>,
        s2: &dyn Shape,
        pos2: &Isometry3<f32>,
        prediction: f32,
        edge_filter: &EdgeFilter,
        vertex_filter: &VertexFilter,
        out: &mut Vec<(Contact, u32)>,
    ) {
        let pos1 = Isometry3::from(coords.cast::<f32>());
        // Use the local space of pos2.
        let pos12 = pos1.inv_mul(pos2);

        // Find new collisions
        let ls_aabb2 = s2.compute_aabb(&pos12).loosened(prediction);

        let mut leaf_callback = |i: &_| {
            let triangle = self.mesh.triangle(*i);
            if let Ok(Some(mut c)) =
                DefaultQueryDispatcher.contact(&pos12, &triangle, s2, prediction)
            {
                if c.dist <= prediction {
                    let (_, feature_id) = triangle
                        .project_local_point_and_get_feature(&pos12.transform_point(&c.point2));

                    let filtered_to_triangle_normal = self.features.get(&(*i, feature_id)).map_or(
                        false,
                        |&feature| match feature {
                            CompoundHullFeature::Edge(edge) => !edge_filter
                                .edge_exists(SortedPair::new(edge.0 + coords, edge.1 + coords)),
                            CompoundHullFeature::Vertex(vertex) => {
                                !vertex_filter.vertex_exists(vertex + coords)
                            }
                        },
                    );

                    if filtered_to_triangle_normal {
                        let triangle_normal = self.mesh.triangle(*i).normal().unwrap();
                        c.normal1 = triangle_normal;
                        c.normal2 = -triangle_normal;
                    }

                    // let triangle_normal = self.mesh.triangle(*i).normal().unwrap();
                    // let is_front_face = triangle_normal.dot(&c.normal1) > 0.;
                    // if c.dist > 0. {
                    //     c.normal1 = triangle_normal;
                    //     c.normal2 = -triangle_normal;
                    // } else {
                    //     c.normal1 = triangle_normal;
                    //     c.normal2 = -triangle_normal;
                    // }

                    c.transform_by_mut(&pos1, pos2);

                    out.push((c, *i));
                }
            }

            true
        };

        let mut visitor = BoundingVolumeIntersectionsVisitor::new(&ls_aabb2, &mut leaf_callback);
        self.mesh.qbvh().traverse_depth_first(&mut visitor);
    }
}
