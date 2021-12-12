use std::{collections::BTreeSet, marker::PhantomData};

use hv::{math, prelude::*};

use crate::render::wireframe::Vertex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct Edge {
    pub v0: u16,
    pub v1: u16,
}

impl Edge {
    pub fn new(v0: u16, v1: u16) -> Self {
        let (&v0, &v1) = math::partial_sort2(&v0, &v1).unwrap();
        Self { v0, v1 }
    }
}

pub type TriangleMesh = IndexedMesh<Triangle>;

pub type TriangleStripMesh = IndexedMesh<TriangleStrip>;

pub type TriangleFanMesh = IndexedMesh<TriangleFan>;

pub type LineMesh = IndexedMesh<Line>;

pub type LineStripMesh = IndexedMesh<LineStrip>;

pub type LineLoopMesh = IndexedMesh<LineLoop>;

pub type PointMesh = IndexedMesh<Point>;

pub enum Triangle {}
pub enum TriangleStrip {}
pub enum TriangleFan {}
pub enum Line {}
pub enum LineStrip {}
pub enum LineLoop {}
pub enum Point {}

pub trait PrimitiveMode: sealed::PrimitiveMode {}
impl<T: sealed::PrimitiveMode> PrimitiveMode for T {}

pub struct IndexedMesh<P: PrimitiveMode> {
    vertices: Vec<Vertex>,
    indices: Vec<u16>,
    primitive_restart: Option<u16>,
    _phantom: PhantomData<P>,
}

impl<P: PrimitiveMode> Default for IndexedMesh<P> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P: PrimitiveMode> IndexedMesh<P> {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            primitive_restart: None,
            _phantom: PhantomData,
        }
    }

    /// Push a single vertex and get back its index.
    pub fn push_vertex(&mut self, vertex: Vertex) -> u16 {
        let index = self.vertices.len();
        self.vertices.push(vertex);
        index as u16
    }

    pub fn vertices(&self) -> &[Vertex] {
        &self.vertices
    }

    pub fn indices(&self) -> &[u16] {
        &self.indices
    }

    pub fn primitive_restart(&self) -> Option<u16> {
        self.primitive_restart
    }
}

impl IndexedMesh<Line> {
    pub fn line_count(&self) -> usize {
        self.indices.len() / 2
    }

    pub fn push_line(&mut self, a: u16, b: u16) {
        self.indices.extend([a, b]);
    }
}

impl IndexedMesh<Triangle> {
    /// How many triangles will we get by drawing this mesh?
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Push a single triangle given an ordered triple of vertex indices.
    pub fn push_triangle(&mut self, vs: [u16; 3]) {
        self.indices.extend(vs);
    }

    /// Push a pair of triangles forming a quad, given an ordered quadruple of vertex indices.
    ///
    /// `swap_order` causes the winding order to be reversed from what it would be otherwise.
    pub fn push_quad(&mut self, vs: [u16; 4], swap_order: bool) {
        if !swap_order {
            self.indices
                .extend([vs[0], vs[1], vs[2], vs[0], vs[2], vs[3]]);
        } else {
            self.indices
                .extend([vs[0], vs[2], vs[3], vs[1], vs[2], vs[3]]);
        }
    }
}

impl IndexedMesh<Triangle> {
    /// Calculate normals ONLY for "provoking" vertices of this triangle mesh. This is useful for
    /// flat shading for wireframes and such, but not for more general "smooth" normals. It works
    /// with the wireframe vertex/fragment shaders, which expect flat normal data.
    ///
    /// This method may invalidate some vertex indices, as different triangles cannot share the same
    /// provoking vertex. If a repeated provoking vertex is found, we first try rotating the
    /// triangle to get a different provoking vertex; if all provoking vertices are taken, we must
    /// duplicate a vertex, use it to carry the new normal, and then replace the original index we
    /// duplicated in the triangle. As such, this method may add vertices to the mesh, but it will
    /// add at most ~1/3 the number of vertices the mesh started with, in the very worst of worst
    /// cases.
    pub fn calculate_flat_normals(&mut self) {
        let mut provoking_set = BTreeSet::new();

        for is in self.indices.chunks_exact_mut(3) {
            let p0 = Vector3::from(self.vertices[is[0] as usize].position);
            let p1 = Vector3::from(self.vertices[is[1] as usize].position);
            let p2 = Vector3::from(self.vertices[is[2] as usize].position);
            let a = p1 - p0;
            let b = p2 - p0;
            let normal = a.cross(&b).normalize();

            if provoking_set.insert(is[2]) {
                self.vertices[is[2] as usize].normal = normal.into();
            } else if provoking_set.insert(is[1]) {
                self.vertices[is[1] as usize].normal = normal.into();
                is.rotate_right(1);
            } else if provoking_set.insert(is[0]) {
                self.vertices[is[0] as usize].normal = normal.into();
                is.rotate_left(1);
            } else {
                // relabel is[2]
                let old_is2 = is[2];
                let mut i2 = self.vertices[old_is2 as usize];
                i2.normal = normal.into();
                let new_is2 = self.vertices.len() as u16;
                is[2] = new_is2;
                self.vertices.push(i2);
            }
        }
    }
}

impl IndexedMesh<Triangle> {
    pub fn push_icosahedron(&mut self, center: Vector3<f32>, radius: f32) {
        let t = (1. + (5.).sqrt()) / 2.;
        let v = Vertex::from;

        // Vertices
        let vs = [
            self.push_vertex(v(radius * (center + Vector3::new(-1., t, 0.0)).normalize())),
            self.push_vertex(v(radius * (center + Vector3::new(1.0, t, 0.0)).normalize())),
            self.push_vertex(v(radius * (center + Vector3::new(-1., -t, 0.0)).normalize())),
            self.push_vertex(v(radius * (center + Vector3::new(1.0, -t, 0.0)).normalize())),
            self.push_vertex(v(radius * (center + Vector3::new(0.0, -1., t)).normalize())),
            self.push_vertex(v(radius * (center + Vector3::new(0.0, 1.0, t)).normalize())),
            self.push_vertex(v(radius * (center + Vector3::new(0.0, -1., -t)).normalize())),
            self.push_vertex(v(radius * (center + Vector3::new(0.0, 1.0, -t)).normalize())),
            self.push_vertex(v(radius * (center + Vector3::new(t, 0.0, -1.)).normalize())),
            self.push_vertex(v(radius * (center + Vector3::new(t, 0.0, 1.0)).normalize())),
            self.push_vertex(v(radius * (center + Vector3::new(-t, 0.0, -1.)).normalize())),
            self.push_vertex(v(radius * (center + Vector3::new(-t, 0.0, 1.0)).normalize())),
        ];

        // Faces
        self.push_triangle([vs[0], vs[11], vs[5]]);
        self.push_triangle([vs[0], vs[5], vs[1]]);
        self.push_triangle([vs[0], vs[1], vs[7]]);
        self.push_triangle([vs[0], vs[7], vs[10]]);
        self.push_triangle([vs[0], vs[10], vs[11]]);
        self.push_triangle([vs[1], vs[5], vs[9]]);
        self.push_triangle([vs[5], vs[11], vs[4]]);
        self.push_triangle([vs[11], vs[10], vs[2]]);
        self.push_triangle([vs[10], vs[7], vs[6]]);
        self.push_triangle([vs[7], vs[1], vs[8]]);
        self.push_triangle([vs[3], vs[9], vs[4]]);
        self.push_triangle([vs[3], vs[4], vs[2]]);
        self.push_triangle([vs[3], vs[2], vs[6]]);
        self.push_triangle([vs[3], vs[6], vs[8]]);
        self.push_triangle([vs[3], vs[8], vs[9]]);
        self.push_triangle([vs[4], vs[9], vs[5]]);
        self.push_triangle([vs[2], vs[4], vs[11]]);
        self.push_triangle([vs[6], vs[2], vs[10]]);
        self.push_triangle([vs[8], vs[6], vs[7]]);
        self.push_triangle([vs[9], vs[8], vs[1]]);
    }
}

impl IndexedMesh<TriangleStrip> {
    pub fn push_triangle(&mut self, indices: [u16; 3]) {
        let index_slice = self.indices.get(self.indices.len() - 3..);
        if matches!(index_slice, Some(sliced) if &indices[0..2] == sliced) {
            self.indices.push(indices[2]);
        } else {
            let pr = *self.primitive_restart.get_or_insert(u16::MAX);
            self.indices.push(pr);
            self.indices.extend(indices);
        }
    }

    pub fn push_index(&mut self, index: u16) {
        self.indices.push(index);
    }

    pub fn extend_indices(&mut self, indices: impl IntoIterator<Item = u16>) {
        self.indices.extend(indices);
    }
}

impl IndexedMesh<TriangleFan> {
    pub fn push_triangle(&mut self, indices: [u16; 3]) {
        let last = self.indices.last();
        if matches!(last, Some(&last) if indices[0] == self.indices[0] && indices[1] == last) {
            self.indices.push(indices[2]);
        } else {
            let pr = *self.primitive_restart.get_or_insert(u16::MAX);
            self.indices.push(pr);
            self.indices.extend(indices);
        }
    }

    pub fn push_index(&mut self, index: u16) {
        self.indices.push(index);
    }

    pub fn extend_indices(&mut self, indices: impl IntoIterator<Item = u16>) {
        self.indices.extend(indices);
    }
}

mod sealed {
    use super::*;

    pub trait PrimitiveMode {}
    impl PrimitiveMode for Triangle {}
    impl PrimitiveMode for TriangleStrip {}
    impl PrimitiveMode for TriangleFan {}
    impl PrimitiveMode for Line {}
    impl PrimitiveMode for LineStrip {}
    impl PrimitiveMode for LineLoop {}
    impl PrimitiveMode for Point {}
}
