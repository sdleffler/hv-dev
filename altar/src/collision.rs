use std::fmt;
use std::{collections::HashMap, sync::Arc};

use hv::prelude::*;
use parry3d::{
    bounding_volume::BoundingVolume,
    query::{
        visitors::BoundingVolumeIntersectionsVisitor, Contact, DefaultQueryDispatcher, PointQuery,
        QueryDispatcher, TOI,
    },
    shape::{FeatureId, Segment, Shape, TriMesh},
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
    Face(HullFacet),
    Edge(SortedPair<Exact>),
    Vertex(Exact),
}

impl CompoundHullFeature {
    pub fn unwrap_face(&self) -> &HullFacet {
        match self {
            Self::Face(facet) => facet,
            _ => unreachable!(),
        }
    }
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

        // Our winding order matches Parry's when it comes to normal calculation.
        for facet in hull.facets() {
            match facet {
                HullFacet::Triangle([a, b, c]) => {
                    let subshape_id = indices.len() as u32;
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

                    features.insert(
                        (subshape_id, FeatureId::Face(0)),
                        CompoundHullFeature::Face(facet),
                    );

                    indices.push([v(a), v(b), v(c)]);
                }
                HullFacet::Rectangle([a, b, c, d]) => {
                    let subshape_id = indices.len() as u32;
                    for (i, v) in [a, b, c, d].into_iter().enumerate() {
                        features.insert(
                            (subshape_id, FeatureId::Vertex(i as u32)),
                            CompoundHullFeature::Vertex(v),
                        );

                        features.insert(
                            (subshape_id + 1, FeatureId::Vertex(i as u32)),
                            CompoundHullFeature::Vertex(v),
                        );
                    }

                    for (i, e) in [(a, b), (b, c), (a, c), (c, d), (d, a)]
                        .into_iter()
                        .enumerate()
                    {
                        features.insert(
                            (subshape_id, FeatureId::Edge(i as u32)),
                            CompoundHullFeature::Edge(SortedPair::new(e.0, e.1)),
                        );
                    }

                    for (i, e) in [(a, c), (c, d), (a, d), (a, b), (b, c)]
                        .into_iter()
                        .enumerate()
                    {
                        features.insert(
                            (subshape_id + 1, FeatureId::Edge(i as u32)),
                            CompoundHullFeature::Edge(SortedPair::new(e.0, e.1)),
                        );
                    }

                    features.insert(
                        (subshape_id, FeatureId::Face(0)),
                        CompoundHullFeature::Face(facet),
                    );
                    features.insert(
                        (subshape_id + 1, FeatureId::Face(0)),
                        CompoundHullFeature::Face(facet),
                    );

                    indices.push([v(a), v(b), v(c)]);
                    indices.push([v(a), v(c), v(d)]);
                }
            }
        }

        let mesh = TriMesh::new(vertices, indices);

        Self { mesh, features }
    }

    /// Perform a contact check by doing a roundabout SAT-like construction.
    ///
    /// This method checks for contacts against every triangle in the mesh, and when it finds
    /// collisions, it looks to see what features of the triangle they occurred on. If the feature
    /// that parry is trying to return is *ignored* - like a redundant edge of a quad which has been
    /// split into two triangles, or an edge between two atoms which is shared and will produce
    /// spurious "cracks" if collided against, or a vertex shared by collinear edges - then the
    /// normal of the contact is forcibly altered to be the normal of the triangle rather than the
    /// original contact normal of the feature.
    ///
    /// This filtering is done through the soft-edge [`EdgeFilter`] and [`VertexFilter`], which
    /// should be generated by an [`AtomMap`](crate::lattice::atom_map::AtomMap).
    #[allow(clippy::too_many_arguments)]
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
                // let triangle_normal = self.mesh.triangle(*i).normal().unwrap();
                let triangle_normal = self.features[&(*i, FeatureId::Face(0))]
                    .unwrap_face()
                    .normal();
                if c.dist <= prediction && triangle_normal.dot(&c.normal1) > 0. {
                    // let (_, feature_id) = triangle
                    //     .project_local_point_and_get_feature(&pos12.transform_point(&c.point2));
                    let (_, feature_id) = triangle.project_local_point_and_get_feature(
                        &(c.point1 + c.normal1.into_inner() * 0.01),
                    );

                    match self.features.get(&(*i, feature_id)) {
                        Some(&CompoundHullFeature::Edge(edge))
                            if !edge_filter
                                .edge_exists(SortedPair::new(edge.0 + coords, edge.1 + coords)) =>
                        {
                            c.normal1 = triangle_normal;
                            c.normal2 = -triangle_normal;

                            // println!(
                            //     "Filtered edge {:?} (subfeature {}, coords: {}) with normal {}\n\tsegment {} => {}, dist {}",
                            //     feature_id,
                            //     i,
                            //     Point3::from(coords),
                            //     Point3::from(c.normal1.into_inner()),
                            //     edge.0 + coords,
                            //     edge.1 + coords,
                            //     c.dist,
                            // );
                        }
                        Some(&CompoundHullFeature::Vertex(vertex))
                            if !vertex_filter.vertex_exists(vertex + coords) =>
                        {
                            let vertex_id = feature_id.unwrap_vertex();
                            // The two extra indices on each of these are in case that the feature
                            // is actually a rectangle, in which case we encode two missing edges as
                            // "extra" edges in the compound hull feature map.
                            let potential_edges = match vertex_id {
                                // Vertex 0 is vertex A, meaning potential edges are AB/AC.
                                0 => [0, 2, 3, 4],
                                // Vertex B - AB/BC.
                                1 => [0, 1, 3, 4],
                                // Vertex C - AC/BC.
                                2 => [1, 2, 3, 4],
                                _ => unreachable!(),
                            };

                            let mut replacement = None::<Contact>;
                            for edge in potential_edges {
                                let chf = match self.features.get(&(*i, FeatureId::Edge(edge))) {
                                    Some(&CompoundHullFeature::Edge(edge)) => edge,
                                    _ => continue,
                                };

                                // println!(
                                //     "Testing potential replacement edge: {} => {}",
                                //     chf.0 + coords,
                                //     chf.1 + coords
                                // );

                                if edge_filter
                                    .edge_exists(SortedPair::new(chf.0 + coords, chf.1 + coords))
                                {
                                    // HACK: we want to do this collision against a "line", not a
                                    // segment! Because a segment's ends will matter. So we lengthen
                                    // the segment in the hope that this will provide a reasonable
                                    // result.
                                    let (a, b) = (chf.0.to_f32(), chf.1.to_f32());
                                    let d = b - a;

                                    // Making it eight times longer in each direction should be
                                    // good, right?
                                    const SEGMENT_LENGTH_FUDGE_FACTOR: f32 = 8.;

                                    let segment = Segment::new(
                                        a - d * SEGMENT_LENGTH_FUDGE_FACTOR,
                                        b + d * SEGMENT_LENGTH_FUDGE_FACTOR,
                                    );
                                    let maybe_new_contact = DefaultQueryDispatcher
                                        .contact(&pos12, &segment, s2, prediction)
                                        .unwrap();

                                    if let Some(new_contact) = maybe_new_contact {
                                        let do_replace = replacement.map_or(true, |last_contact| {
                                            new_contact.dist < last_contact.dist
                                        });
                                        if do_replace {
                                            // println!("replaced!");
                                            replacement = Some(new_contact);
                                        }
                                    }
                                }
                            }

                            if let Some(replacement) = replacement {
                                c = replacement;
                            } else {
                                c.normal1 = triangle_normal;
                                c.normal2 = -triangle_normal;
                            }

                            // println!(
                            //     "Filtered vertex {:?} (subfeature {}, coords: {}) with normal {} and cosine factor {} against triangle normal {} (replaced: {})\n\tpoint {}",
                            //     feature_id,
                            //     i,
                            //     Point3::from(coords),
                            //     Point3::from(c.normal1.into_inner()),
                            //     c.normal1.dot(&triangle_normal),
                            //     Point3::from(triangle_normal.into_inner()),
                            //     replacement.is_some(),
                            //     vertex + coords,
                            // );
                        }
                        _ => {
                            // match feature_id {
                            //     FeatureId::Face(id) => {
                            //         // println!(
                            //         //     "Significant: unfiltered face {} (subfeature {}, coords: {}) with normal {}",
                            //         //     id,
                            //         //     i,
                            //         //     Point3::from(coords),
                            //         //     Point3::from(c.normal1.into_inner()),
                            //         // );
                            //     }
                            //     FeatureId::Edge(id) => {
                            //         let chf_edge = match self.features[&(*i, feature_id)] {
                            //             CompoundHullFeature::Edge(e) => e,
                            //             _ => unreachable!(),
                            //         };

                            //         println!(
                            //             "Significant: unfiltered edge {} (subfeature {}, coords: {}) with normal {}\n\tsegment {} => {}",
                            //             id,
                            //             i,
                            //             Point3::from(coords),
                            //             Point3::from(c.normal1.into_inner()),
                            //             chf_edge.0 + coords,
                            //             chf_edge.1 + coords,
                            //         );
                            //     }
                            //     FeatureId::Vertex(id) => {
                            //         println!(
                            //             "Significant: unfiltered vertex {} (coords: {})",
                            //             id,
                            //             Point3::from(coords)
                            //         );
                            //     }
                            //     _ => {}
                            // };
                        }
                    };

                    // Recalculate the distance, given that the normals may have changed.
                    c.dist = (c.point2 - c.point1).dot(&c.normal1);
                    if c.dist <= prediction {
                        c.transform_by_mut(&pos1, pos2);
                        out.push((c, *i));
                    }
                }
            }

            true
        };

        let mut visitor = BoundingVolumeIntersectionsVisitor::new(&ls_aabb2, &mut leaf_callback);
        self.mesh.qbvh().traverse_depth_first(&mut visitor);
    }

    pub fn time_of_impact(
        &self,
        coords: &Vector3<i32>,
        pos2: &Isometry3<f32>,
        vel2: &Vector3<f32>,
        s2: &dyn Shape,
        max_toi: f32,
    ) -> Option<TOI> {
        let pos1 = Isometry3::from(coords.cast::<f32>());
        let pos12 = pos1.inv_mul(pos2);
        parry3d::query::details::time_of_impact_composite_shape_shape(
            &DefaultQueryDispatcher,
            &pos12,
            vel2,
            &self.mesh,
            s2,
            max_toi,
        )
    }
}
