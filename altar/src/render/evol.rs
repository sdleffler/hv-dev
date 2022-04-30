use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
};

use decorum::Total;
use genmesh::{
    generators::{Cube, Cylinder, IndexedPolygon, SharedVertex, SphereUv},
    MapToVertices, Triangulate,
};
use glyph::{
    ab_glyph::{FontArc, PxScale},
    BuiltInLineBreaker, Extra, FontId, Layout, Section, Text,
};
use hv::prelude::*;
use luminance::{
    backend::{
        color_slot::ColorSlot,
        depth_stencil_slot::DepthStencilSlot,
        pipeline::{Pipeline as PipelineBackend, PipelineTexture},
        shader::{ShaderData as ShaderDataBackend, Uniformable},
        tess::{
            IndexSlice as IndexSliceBackend, InstanceSlice as InstanceSliceBackend,
            Tess as TessBackend, VertexSlice as VertexSliceBackend,
        },
        tess_gate::TessGate as TessGateBackend,
        texture::Texture as TextureBackend,
    },
    blending::{Blending, Equation, Factor},
    context::GraphicsContext,
    framebuffer::Framebuffer,
    pipeline::{Pipeline, PipelineState, TextureBinding},
    pixel::{NormRGBA8UI, NormUnsigned},
    render_state::RenderState,
    shader::{types::Mat44, Program, Uniform},
    shading_gate::ShadingGate,
    tess::{
        Instances, InstancesMut, Interleaved, Mode, Tess, TessMapError, TessView, TessViewError,
        View,
    },
    texture::{Dim2, Sampler, TexelUpload, Texture},
    UniformInterface, Vertex,
};
use luminance_glyph::{GlyphBrush, GlyphBrushBackend, GlyphBrushBuilder};
use lyon::{
    lyon_tessellation::{
        BuffersBuilder, FillTessellator, FillVertex, FillVertexConstructor, StrokeTessellator,
        StrokeVertex, StrokeVertexConstructor, VertexBuffers,
    },
    path::{AttributeStore, EndpointId, Polygon, Position},
};
use thunderdome::{Arena, Index};

use crate::render::{
    pipeline::semantics::{
        InstanceColor, InstanceSource, InstanceTCol1, InstanceTCol2, InstanceTCol3, InstanceTCol4,
        InstanceTColumns, VertexColor, VertexPosition, VertexSemantics, VertexUv,
    },
    Color,
};

pub use lyon::tessellation::{
    FillOptions, FillRule, LineCap, LineJoin, Orientation, StrokeOptions,
};

pub use luminance_glyph as glyph;

pub trait EvolBackend:
    for<'a> IndexSliceBackend<'a, VertexData, u16, InstanceData, Interleaved>
    + for<'a> InstanceSliceBackend<'a, VertexData, u16, InstanceData, Interleaved, InstanceData>
    + PipelineBackend<Dim2>
    + PipelineTexture<Dim2, NormRGBA8UI>
    + ShaderDataBackend<Mat44<f32>>
    + TessBackend<(), (), (), Interleaved>
    + TessBackend<VertexData, u16, InstanceData, Interleaved>
    + TessGateBackend<VertexData, u16, InstanceData, Interleaved>
    + TextureBackend<Dim2, NormRGBA8UI>
    + for<'a> VertexSliceBackend<'a, VertexData, u16, InstanceData, Interleaved, VertexData>
    + for<'a> Uniformable<'a, f32, Target = f32>
    + GlyphBrushBackend
{
}

impl<B> EvolBackend for B where
    B: for<'a> IndexSliceBackend<'a, VertexData, u16, InstanceData, Interleaved>
        + for<'a> InstanceSliceBackend<'a, VertexData, u16, InstanceData, Interleaved, InstanceData>
        + PipelineBackend<Dim2>
        + PipelineTexture<Dim2, NormRGBA8UI>
        + ShaderDataBackend<Mat44<f32>>
        + TessBackend<(), (), (), Interleaved>
        + TessBackend<VertexData, u16, InstanceData, Interleaved>
        + TessGateBackend<VertexData, u16, InstanceData, Interleaved>
        + TextureBackend<Dim2, NormRGBA8UI>
        + for<'a> VertexSliceBackend<'a, VertexData, u16, InstanceData, Interleaved, VertexData>
        + for<'a> Uniformable<'a, f32, Target = f32>
        + GlyphBrushBackend
{
}

pub const DEFAULT_LINE_THICKNESS: f32 = 0.001;

#[derive(Debug, Clone, Copy)]
pub enum TessOptions {
    Fill(FillOptions),
    Stroke(StrokeOptions),
}

impl TessOptions {
    pub fn fill() -> Self {
        Self::Fill(FillOptions::default())
    }

    pub fn stroke(width: f32) -> Self {
        Self::Stroke(StrokeOptions::default().with_line_width(width))
    }
}

#[derive(Debug, Clone, Copy, Vertex)]
#[vertex(sem = "VertexSemantics")]
pub struct VertexData {
    pub position: VertexPosition,
    #[vertex(normalized = true)]
    pub color: VertexColor,
    pub uv: VertexUv,
}

impl Default for VertexData {
    fn default() -> Self {
        Self {
            position: VertexPosition::new([0.; 3]),
            color: VertexColor::new([1.; 4]),
            uv: VertexUv::new([0.; 2]),
        }
    }
}

impl From<Vertex> for VertexData {
    fn from(v: Vertex) -> Self {
        Self {
            position: VertexPosition::new([v.xy.x, v.xy.y, v.rgbazuv[4]]),
            color: VertexColor::new([v.rgbazuv[0], v.rgbazuv[1], v.rgbazuv[2], v.rgbazuv[3]]),
            uv: VertexUv::new([v.rgbazuv[5], v.rgbazuv[6]]),
        }
    }
}

impl VertexData {
    pub fn xyz(point: Point3<f32>) -> Self {
        Self {
            position: VertexPosition::new(point.into()),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, Vertex)]
#[vertex(sem = "VertexSemantics", instanced = "true")]
pub struct InstanceData {
    #[vertex(normalized = true)]
    color: InstanceColor,
    source: InstanceSource,
    tcol1: InstanceTCol1,
    tcol2: InstanceTCol2,
    tcol3: InstanceTCol3,
    tcol4: InstanceTCol4,
}

impl Default for InstanceData {
    fn default() -> Self {
        let InstanceTColumns {
            tcol1,
            tcol2,
            tcol3,
            tcol4,
        } = Matrix4::identity().into();

        Self {
            color: InstanceColor::new(Color::WHITE.into()),
            source: InstanceSource::new([0., 0., 0., 0.]),
            tcol1,
            tcol2,
            tcol3,
            tcol4,
        }
    }
}

impl InstanceData {
    pub fn color(&self) -> Color {
        let components: [f32; 4] = *self.color;
        Color::from(components)
    }

    pub fn set_color(&mut self, color: &Color) -> &mut Self {
        self.color = InstanceColor::new((*color).into());
        self
    }

    pub fn uv_origin(&self) -> Point2<f32> {
        Point2::new(self.source[0], self.source[1])
    }

    pub fn set_uv_origin(&mut self, uv_origin: &Point2<f32>) -> &mut Self {
        self.source[0] = uv_origin.x;
        self.source[1] = uv_origin.y;
        self
    }

    pub fn uv_extents(&self) -> Vector2<f32> {
        Vector2::new(self.source[2], self.source[3])
    }

    pub fn set_uv_extents(&mut self, uv_extents: &Vector2<f32>) -> &mut Self {
        self.source[2] = uv_extents.x;
        self.source[3] = uv_extents.y;
        self
    }

    pub fn tx(&self) -> Matrix4<f32> {
        InstanceTColumns {
            tcol1: self.tcol1,
            tcol2: self.tcol2,
            tcol3: self.tcol3,
            tcol4: self.tcol4,
        }
        .into()
    }

    pub fn set_tx(&mut self, matrix: &Matrix4<f32>) -> &mut Self {
        let tcols = InstanceTColumns::from(*matrix);
        self.tcol1 = tcols.tcol1;
        self.tcol2 = tcols.tcol2;
        self.tcol3 = tcols.tcol3;
        self.tcol4 = tcols.tcol4;
        self
    }

    pub fn to_instance(&self) -> Instance {
        Instance {
            color: self.color(),
            uv_origin: self.uv_origin(),
            uv_extents: self.uv_extents(),
            tx: self.tx(),
        }
    }
}

impl From<Instance> for InstanceData {
    fn from(instance: Instance) -> Self {
        let tcols = InstanceTColumns::from(instance.tx);
        let p = instance.uv_origin;
        let v = instance.uv_extents;
        Self {
            color: InstanceColor::new(instance.color.into()),
            source: InstanceSource::new([p.x, p.y, v.x, v.y]),
            tcol1: tcols.tcol1,
            tcol2: tcols.tcol2,
            tcol3: tcols.tcol3,
            tcol4: tcols.tcol4,
        }
    }
}

#[derive(Debug, UniformInterface)]
pub struct Uniforms {
    #[uniform(unbound, name = "u_LineThickness")]
    pub line_thickness: Uniform<f32>,
    #[uniform(unbound, name = "u_ViewProjection")]
    pub view_projection: Uniform<Mat44<f32>>,
    #[uniform(unbound, name = "u_Texture")]
    pub texture: Uniform<TextureBinding<Dim2, NormUnsigned>>,
}

/// A vertex, suitable for *2D* tessellation with the [`lyon`] crate.
///
/// This actually consists of three parts, a 3D vector, an RGBA color in sRGB color space, and a UV
/// coordinate. However, the Z component of that vector is treated as not a full-fledged part of
/// the vertex but rather as a vertex attribute like the RGBA/UV components. For the purposes of
/// tessellation, this is useful: you can tessellate a 2D shape, ensuring that its projection
/// (ignoring the Z axis, looking at just the XY axes) will play nice with the tessellator while
/// preserving the Z coordinate (if it matters) for some later use. This is of course limited, but
/// the Z coordinate is there the whole time anyways, so we might as well let you use it here and
/// allow it to be interpolated linearly as per the semantics of vertex attributes in Lyon.
#[derive(Debug, Clone, Copy)]
pub struct Vertex {
    xy: Point2<f32>,
    rgbazuv: [f32; 7],
}

impl Vertex {
    pub fn new(position: Point3<f32>, color: Color, uv: Point2<f32>) -> Self {
        Self {
            xy: position.xy(),
            rgbazuv: [color.r, color.g, color.b, color.a, position.z, uv.x, uv.y],
        }
    }

    pub fn xy_uv(x: f32, y: f32, u: f32, v: f32) -> Self {
        Self::new(Point3::new(x, y, 0.), Color::WHITE, Point2::new(u, v))
    }

    pub fn xy(x: f32, y: f32) -> Self {
        Self::new(Point3::new(x, y, 0.), Color::WHITE, Point2::origin())
    }

    pub fn xyz(x: f32, y: f32, z: f32) -> Self {
        Self::new(Point3::new(x, y, z), Color::WHITE, Point2::origin())
    }
}

impl Position for Vertex {
    fn position(&self) -> lyon::math::Point {
        lyon::math::point(self.xy.x, self.xy.y)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Instance {
    /// A color, in sRGB color space, for this instance.
    pub color: Color,
    /// The origin of the UV coordinate space to use for this instance. This defines the top-left
    /// corner of a "source rectangle" used by the instance being rendered here.
    pub uv_origin: Point2<f32>,
    /// The extents of the UV coordinate space to use for this instance. This defines the width and
    /// height of a "source rectangle" used by the instance being rendered here.
    pub uv_extents: Vector2<f32>,
    /// The model transform for this instance.
    pub tx: Matrix4<f32>,
}

impl Default for Instance {
    fn default() -> Self {
        Self::new()
    }
}

impl Instance {
    #[inline]
    pub fn new() -> Self {
        Self {
            color: Color::WHITE,
            uv_origin: Point2::origin(),
            uv_extents: Vector2::repeat(1.),
            tx: Matrix4::identity(),
        }
    }

    #[inline(always)]
    pub fn with_color(self, color: Color) -> Self {
        Self { color, ..self }
    }

    #[inline(always)]
    pub fn with_uv_origin(self, uv_origin: Point2<f32>) -> Self {
        Self { uv_origin, ..self }
    }

    #[inline(always)]
    pub fn with_uv_extents(self, uv_extents: Vector2<f32>) -> Self {
        Self { uv_extents, ..self }
    }

    #[inline(always)]
    pub fn with_tx(self, tx: Matrix4<f32>) -> Self {
        Self { tx, ..self }
    }
}

struct VertexSlice<'a>(&'a [Vertex]);

impl<'a> AttributeStore for VertexSlice<'a> {
    fn get(&self, id: EndpointId) -> &[f32] {
        &self.0[id.to_usize()].rgbazuv
    }

    fn num_attributes(&self) -> usize {
        7
    }
}

#[derive(Debug, Clone, Copy)]
struct Ctor<'a> {
    color: Color,
    z: f32,
    tx: &'a Matrix4<f32>,
}

impl<'a> FillVertexConstructor<VertexData> for Ctor<'a> {
    fn new_vertex(&mut self, mut vertex: FillVertex) -> VertexData {
        let pos = vertex.position();
        let attrs = vertex.interpolated_attributes();
        if !attrs.is_empty() {
            let (&[r, g, b, a, z, u, v], _) = attrs.split_array_ref::<7>();
            let pt = self.tx.transform_point(&Point3::new(pos.x, pos.y, z));
            VertexData {
                position: VertexPosition::new(pt.into()),
                color: VertexColor::new([r, g, b, a]),
                uv: VertexUv::new([u, v]),
            }
        } else {
            let pt = self.tx.transform_point(&Point3::new(pos.x, pos.y, self.z));
            VertexData {
                position: VertexPosition::new(pt.into()),
                color: VertexColor::new(self.color.into()),
                uv: VertexUv::new([0., 0.]),
            }
        }
    }
}

impl<'a> StrokeVertexConstructor<VertexData> for Ctor<'a> {
    fn new_vertex(&mut self, mut vertex: StrokeVertex) -> VertexData {
        let pos = vertex.position();
        let attrs = vertex.interpolated_attributes();
        if !attrs.is_empty() {
            let (&[r, g, b, a, z, u, v], _) = attrs.split_array_ref::<7>();
            let pt = self.tx.transform_point(&Point3::new(pos.x, pos.y, z));
            VertexData {
                position: VertexPosition::new(pt.into()),
                color: VertexColor::new([r, g, b, a]),
                uv: VertexUv::new([u, v]),
            }
        } else {
            let pt = self.tx.transform_point(&Point3::new(pos.x, pos.y, self.z));
            VertexData {
                position: VertexPosition::new(pt.into()),
                color: VertexColor::new(self.color.into()),
                uv: VertexUv::new([0., 0.]),
            }
        }
    }
}

pub struct MeshBuilder {
    buffers: VertexBuffers<VertexData, u16>,
    fill_tessellator: FillTessellator,
    stroke_tessellator: StrokeTessellator,
    tx: TransformStack,
}

impl Default for MeshBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl MeshBuilder {
    pub fn new() -> Self {
        Self {
            buffers: VertexBuffers::new(),
            fill_tessellator: FillTessellator::new(),
            stroke_tessellator: StrokeTessellator::new(),
            tx: TransformStack::new(),
        }
    }

    pub fn push_raw_vertex(&mut self, vertex: VertexData) -> u16 {
        let i = self.buffers.vertices.len();
        self.buffers.vertices.push(vertex);
        i as u16
    }

    pub fn push_vertex(&mut self, mut vertex: VertexData) -> u16 {
        vertex.position = VertexPosition::new(
            self.tx
                .transform_point(&Point3::from(*vertex.position))
                .into(),
        );
        self.push_raw_vertex(vertex)
    }

    pub fn push_triangle(&mut self, indices: [u16; 3]) {
        self.buffers.indices.extend(indices);
    }

    pub fn set_transform(&mut self, tx: &Matrix4<f32>) -> &mut Self {
        *self.tx = *tx;
        self
    }

    pub fn apply_transform(&mut self, tx: &Matrix4<f32>) -> &mut Self {
        *self.tx *= tx;
        self
    }

    /// Push the transform stack (duplicate the current top transform.)
    ///
    /// Useful when combined with [`MeshBuilder::pop`] to "save" transform state.
    pub fn push(&mut self) -> &mut Self {
        self.tx.push();
        self
    }

    /// Pop the transform stack.
    ///
    /// Useful when combined with [`MeshBuilder::push`] to "reload" transform state.
    pub fn pop(&mut self) -> &mut Self {
        self.tx.pop();
        self
    }

    pub fn translate(&mut self, x: f32, y: f32, z: f32) -> &mut Self {
        *self.tx *= Matrix4::new_translation(&Vector3::new(x, y, z));
        self
    }

    pub fn scale(&mut self, x: f32, y: f32, z: f32) -> &mut Self {
        *self.tx *= Matrix4::new_nonuniform_scaling(&Vector3::new(x, y, z));
        self
    }

    pub fn circle(
        &mut self,
        center: &Point3<f32>,
        radius: f32,
        options: &TessOptions,
        color: Color,
    ) -> Result<&mut Self> {
        let output = &mut BuffersBuilder::new(
            &mut self.buffers,
            Ctor {
                color,
                z: center.z,
                tx: &self.tx,
            },
        );

        let tess_result = match options {
            TessOptions::Fill(fill_options) => self.fill_tessellator.tessellate_circle(
                lyon::math::point(center.x, center.y),
                radius,
                fill_options,
                output,
            ),
            TessOptions::Stroke(stroke_options) => self.stroke_tessellator.tessellate_circle(
                lyon::math::point(center.x, center.y),
                radius,
                stroke_options,
                output,
            ),
        };

        if let Err(tess_error) = tess_result {
            Err(anyhow!("tessellation error: {:?}", tess_error))
        } else {
            Ok(self)
        }
    }

    pub fn quad(&mut self, origin: &Point2<f32>, extents: &Vector2<f32>) -> Result<&mut Self> {
        self.polygon(
            &[
                Vertex::xy_uv(origin.x, origin.y, 0., 1.),
                Vertex::xy_uv(origin.x + extents.x, origin.y, 1., 1.),
                Vertex::xy_uv(origin.x + extents.x, origin.y + extents.y, 1., 0.),
                Vertex::xy_uv(origin.x, origin.y + extents.y, 0., 0.),
            ],
            &TessOptions::fill(),
        )
    }

    pub fn polygon(&mut self, vertices: &[Vertex], options: &TessOptions) -> Result<&mut Self> {
        let polygon = Polygon {
            points: vertices,
            closed: true,
        };

        let output = &mut BuffersBuilder::new(
            &mut self.buffers,
            Ctor {
                color: Color::WHITE,
                z: 0.,
                tx: &self.tx,
            },
        );

        let tess_result = match options {
            TessOptions::Fill(fill_options) => self.fill_tessellator.tessellate_with_ids(
                polygon.id_iter(),
                &polygon,
                Some(&VertexSlice(polygon.points)),
                fill_options,
                output,
            ),
            TessOptions::Stroke(stroke_options) => self.stroke_tessellator.tessellate_with_ids(
                polygon.id_iter(),
                &polygon,
                Some(&VertexSlice(polygon.points)),
                stroke_options,
                output,
            ),
        };

        if let Err(tess_error) = tess_result {
            Err(anyhow!("tessellation error: {:?}", tess_error))
        } else {
            Ok(self)
        }
    }

    #[allow(clippy::identity_op)]
    pub fn cuboid(&mut self, half_extents: &Vector3<f32>, color: &Color) -> Result<&mut Self> {
        let generator = Cube::new();
        let shared_vertices = generator
            .shared_vertex_iter()
            .map(|v| {
                Vertex::new(
                    Point3::new(
                        v.pos.x * half_extents.x,
                        v.pos.y * half_extents.y,
                        v.pos.z * half_extents.z,
                    ),
                    *color,
                    Point2::origin(),
                )
            })
            .map(VertexData::from)
            .collect::<Vec<_>>();

        for triangle in generator
            .indexed_polygon_iter()
            .triangulate()
            .vertex(|i| shared_vertices[i])
        {
            let offset_i = u16::try_from(self.buffers.vertices.len()).unwrap();
            let verts = [triangle.x, triangle.y, triangle.z]
                .into_iter()
                .map(|mut v| {
                    let mut pt = Point3::from(*v.position);
                    pt = self.tx.transform_point(&pt);
                    v.position = VertexPosition::new(pt.into());
                    v
                });
            self.buffers.vertices.extend(verts);
            self.buffers.indices.extend(offset_i..=(offset_i + 2));
        }

        Ok(self)
    }

    #[allow(clippy::identity_op)]
    pub fn uv_sphere(&mut self, u: u32, v: u32, radius: f32, color: &Color) -> Result<&mut Self> {
        let generator = SphereUv::new(u as usize, v as usize);
        let shared_vertices = generator
            .shared_vertex_iter()
            .map(|v| {
                Vertex::new(
                    Point3::new(v.pos.x * radius, v.pos.y * radius, v.pos.z * radius),
                    *color,
                    Point2::origin(),
                )
            })
            .map(VertexData::from)
            .collect::<Vec<_>>();

        for triangle in generator
            .indexed_polygon_iter()
            .triangulate()
            .vertex(|i| shared_vertices[i])
        {
            let offset_i = u16::try_from(self.buffers.vertices.len()).unwrap();
            let verts = [triangle.x, triangle.y, triangle.z]
                .into_iter()
                .map(|mut v| {
                    let mut pt = Point3::from(*v.position);
                    pt = self.tx.transform_point(&pt);
                    v.position = VertexPosition::new(pt.into());
                    v
                });
            self.buffers.vertices.extend(verts);
            self.buffers.indices.extend(offset_i..=(offset_i + 2));
        }

        Ok(self)
    }

    #[allow(clippy::identity_op)]
    pub fn uv_hemisphere(
        &mut self,
        u: u32,
        v: u32,
        radius: f32,
        color: &Color,
    ) -> Result<&mut Self> {
        let generator = SphereUv::new(u as usize, 2 * v as usize);
        let shared_vertices = generator
            .shared_vertex_iter()
            .map(|v| {
                Vertex::new(
                    Point3::new(v.pos.x * radius, v.pos.y * radius, v.pos.z * radius),
                    *color,
                    Point2::origin(),
                )
            })
            .map(VertexData::from)
            .collect::<Vec<_>>();

        for triangle in generator
            .indexed_polygon_iter()
            .triangulate()
            .vertex(|i| shared_vertices[i])
        {
            if triangle.x.position[2]
                .min(triangle.y.position[2])
                .min(triangle.z.position[2])
                < 0.
            {
                continue;
            }

            let offset_i = u16::try_from(self.buffers.vertices.len()).unwrap();
            let verts = [triangle.x, triangle.y, triangle.z]
                .into_iter()
                .map(|mut v| {
                    let mut pt = Point3::from(*v.position);
                    pt = self.tx.transform_point(&pt);
                    v.position = VertexPosition::new(pt.into());
                    v
                });
            self.buffers.vertices.extend(verts);
            let indxs = [offset_i + 0, offset_i + 1, offset_i + 2];
            self.buffers.indices.extend(indxs);
        }

        Ok(self)
    }

    #[allow(clippy::identity_op)]
    pub fn cylinder(
        &mut self,
        u: u32,
        half_height: f32,
        radius: f32,
        color: &Color,
    ) -> Result<&mut Self> {
        let generator = Cylinder::new(u as usize);
        let shared_vertices = generator
            .shared_vertex_iter()
            .map(|v| {
                Vertex::new(
                    Point3::new(v.pos.x * radius, v.pos.y * radius, v.pos.z * half_height),
                    *color,
                    Point2::origin(),
                )
            })
            .map(VertexData::from)
            .collect::<Vec<_>>();

        for triangle in generator
            .indexed_polygon_iter()
            .triangulate()
            .vertex(|i| shared_vertices[i])
        {
            let offset_i = u16::try_from(self.buffers.vertices.len()).unwrap();
            let verts = [triangle.x, triangle.y, triangle.z]
                .into_iter()
                .map(|mut v| {
                    let mut pt = Point3::from(*v.position);
                    pt = self.tx.transform_point(&pt);
                    v.position = VertexPosition::new(pt.into());
                    v
                });
            self.buffers.vertices.extend(verts);
            let indxs = [offset_i + 0, offset_i + 1, offset_i + 2];
            self.buffers.indices.extend(indxs);
        }

        Ok(self)
    }

    #[allow(clippy::identity_op)]
    pub fn uncapped_cylinder(
        &mut self,
        u: u32,
        half_height: f32,
        radius: f32,
        color: &Color,
    ) -> Result<&mut Self> {
        let generator = Cylinder::new(u as usize);
        let shared_vertices = generator
            .shared_vertex_iter()
            .map(|v| {
                Vertex::new(
                    Point3::new(v.pos.x * radius, v.pos.y * radius, v.pos.z * half_height),
                    *color,
                    Point2::origin(),
                )
            })
            .map(VertexData::from)
            .collect::<Vec<_>>();

        for triangle in generator
            .indexed_polygon_iter()
            .triangulate()
            .vertex(|i| shared_vertices[i])
        {
            if triangle.x.position[2] == triangle.y.position[2]
                && triangle.x.position[2] == triangle.z.position[2]
            {
                // Cut out the "end caps" - triangles which have the same Z.
                continue;
            }

            let offset_i = u16::try_from(self.buffers.vertices.len()).unwrap();
            let verts = [triangle.x, triangle.y, triangle.z]
                .into_iter()
                .map(|mut v| {
                    let mut pt = Point3::from(*v.position);
                    pt = self.tx.transform_point(&pt);
                    v.position = VertexPosition::new(pt.into());
                    v
                });
            self.buffers.vertices.extend(verts);
            let indxs = [offset_i + 0, offset_i + 1, offset_i + 2];
            self.buffers.indices.extend(indxs);
        }

        Ok(self)
    }

    pub fn uv_capsule(
        &mut self,
        u: u32,
        hv: u32,
        half_height: f32,
        radius: f32,
        color: &Color,
    ) -> Result<&mut Self> {
        self.push()
            .translate(0., 0., half_height)
            .uv_hemisphere(u, hv, radius, color)?
            .pop()
            .push()
            .scale(1., 1., -1.)
            .translate(0., 0., half_height)
            .uv_hemisphere(u, hv, radius, color)?
            .pop()
            .cylinder(u, half_height, radius, color)
    }

    pub fn vertices(&self) -> &[VertexData] {
        &self.buffers.vertices
    }

    pub fn indices(&self) -> &[u16] {
        &self.buffers.indices
    }

    pub fn reset(&mut self) -> &mut Self {
        self.buffers.vertices.clear();
        self.buffers.indices.clear();
        self.tx.clear();
        self
    }

    /// Append one mesh builder into this one, optionally reversing the winding of the triangles.
    pub fn append(&mut self, other: &mut Self, reverse_winding: bool) -> &mut Self {
        self.buffers.vertices.append(&mut other.buffers.vertices);
        if reverse_winding {
            for [i1, i2, i3] in other.buffers.indices.array_chunks::<3>() {
                self.buffers.indices.extend([i1, i3, i2]);
            }
            other.buffers.indices.clear();
        } else {
            self.buffers.indices.append(&mut other.buffers.indices);
        }
        self
    }

    /// Build the mesh and clear the builder's index and vertex buffers.
    pub fn drain<B: EvolBackend>(
        &mut self,
        ctx: &mut impl GraphicsContext<Backend = B>,
        instance_count: usize,
    ) -> Result<Mesh<B>> {
        let mesh = self.build(ctx, instance_count)?;
        self.reset();
        Ok(mesh)
    }

    /// Build the mesh *without* clearing the builder.
    pub fn build<B: EvolBackend>(
        &self,
        ctx: &mut impl GraphicsContext<Backend = B>,
        instance_count: usize,
    ) -> Result<Mesh<B>> {
        let tess = ctx
            .new_tess()
            .set_mode(Mode::Triangle)
            .set_vertices(&*self.buffers.vertices)
            .set_indices(&*self.buffers.indices)
            .set_instances(vec![InstanceData::default(); instance_count])
            .build()?;

        Ok(Mesh {
            tess,
            index_count: self.buffers.indices.len(),
            instance_count,
        })
    }

    /// Build a mesh, attempting to reuse a previous [`Tess`] allocation; then, clear the
    /// vertex/index buffers.
    ///
    /// If the mesh can't be fit into the [`Tess`] allocated by the [`Mesh`] provided, a new `Tess`
    /// is allocated which is strictly bigger than the old one and capable of fitting all required
    /// elements.
    pub fn drain_into<B: EvolBackend>(
        &mut self,
        ctx: &mut impl GraphicsContext<Backend = B>,
        mesh: &mut Mesh<B>,
        instance_count: usize,
    ) -> Result<()> {
        self.build_into(ctx, mesh, instance_count)?;
        self.reset();
        Ok(())
    }

    /// Build a mesh, attempting to reuse a previous [`Tess`] allocation, *without* clearing the
    /// vertex/index buffers.
    ///
    /// If the mesh can't be fit into the [`Tess`] allocated by the [`Mesh`] provided, a new `Tess`
    /// is allocated which is strictly bigger than the old one and capable of fitting all required
    /// elements.
    pub fn build_into<B: EvolBackend>(
        &self,
        ctx: &mut impl GraphicsContext<Backend = B>,
        mesh: &mut Mesh<B>,
        instance_count: usize,
    ) -> Result<()> {
        if self.buffers.vertices.len() > mesh.tess.vert_nb()
            || self.buffers.indices.len() > mesh.tess.idx_nb()
            || instance_count > mesh.tess.inst_nb()
        {
            let new_vert_nb = self.buffers.vertices.len().max(mesh.tess.vert_nb());
            let new_idx_nb = self.buffers.indices.len().max(mesh.tess.idx_nb());
            let new_inst_nb = instance_count.max(mesh.tess.inst_nb());

            let mut vertices = self.buffers.vertices.clone();
            vertices.resize_with(new_vert_nb, VertexData::default);
            let mut indices = self.buffers.indices.clone();
            indices.resize(new_idx_nb, 0);
            let instances = vec![InstanceData::default(); new_inst_nb];

            mesh.tess = ctx
                .new_tess()
                .set_mode(Mode::Triangle)
                .set_vertices(vertices)
                .set_indices(indices)
                .set_instances(instances)
                .build()?;

            mesh.index_count = self.buffers.indices.len();
            mesh.instance_count = instance_count;
        } else {
            self.buffers
                .vertices
                .iter()
                .zip(mesh.tess.vertices_mut()?.iter_mut())
                .for_each(|(src, dst)| *dst = *src);

            self.buffers
                .indices
                .iter()
                .zip(mesh.tess.indices_mut()?.iter_mut())
                .for_each(|(src, dst)| *dst = *src);

            mesh.index_count = self.buffers.indices.len();
            mesh.instance_count = instance_count;
        }

        Ok(())
    }
}

pub struct Mesh<B: EvolBackend> {
    tess: Tess<B, VertexData, u16, InstanceData>,
    index_count: usize,
    instance_count: usize,
}

impl<B: EvolBackend> Mesh<B> {
    pub fn ensure_capacity(
        &mut self,
        context: &mut impl GraphicsContext<Backend = B>,
        instance_capacity: usize,
    ) -> Result<()> {
        // If it's too big... we have to allocate a new tess.
        if instance_capacity > self.tess.inst_nb() {
            let vertices = self.tess.vertices()?.to_vec();
            let indices = self.tess.indices()?.to_vec();
            let mut instances = Vec::with_capacity(instance_capacity);
            instances.extend_from_slice(&*self.tess.instances()?);
            instances.resize_with(instance_capacity, InstanceData::default);
            let new_tess = context
                .new_tess()
                .set_mode(Mode::Triangle)
                .set_vertices(vertices)
                .set_indices(indices)
                .set_instances(instances)
                .build()?;
            self.tess = new_tess;
        }

        Ok(())
    }

    /// Resize the mesh's instance buffer.
    pub fn resize(
        &mut self,
        instance_count: usize,
        context: &mut impl GraphicsContext<Backend = B>,
    ) -> Result<()> {
        self.ensure_capacity(context, instance_count)?;
        self.instance_count = instance_count;

        Ok(())
    }

    /// Clear all instance data.
    pub fn clear(&mut self) {
        self.instance_count = 0;
    }

    /// Is there any room left for more instances?
    pub fn is_full(&self) -> bool {
        self.instance_count == self.instance_capacity()
    }

    /// How many instances can this mesh hold at once?
    pub fn instance_capacity(&self) -> usize {
        self.tess.inst_nb()
    }

    pub fn try_push(&mut self, instance: &Instance) -> Result<()> {
        let index = self.instance_count;
        ensure!(
            self.instance_count < self.tess.inst_nb(),
            "mesh instance buffer is full!"
        );
        self.tess.instances_mut()?[index] = (*instance).into();
        self.instance_count += 1;
        Ok(())
    }

    pub fn try_write(&mut self) -> Result<TryWriteInstances<B>> {
        Ok(TryWriteInstances {
            instances_mut: self.tess.instances_mut()?,
            instance_count: &mut self.instance_count,
        })
    }

    pub fn push(
        &mut self,
        context: &mut impl GraphicsContext<Backend = B>,
        instance: Instance,
    ) -> Result<()> {
        let index = self.instance_count;
        if self.instance_count >= self.tess.inst_nb() {
            self.ensure_capacity(context, self.instance_count.next_power_of_two())?;
        }
        self.tess.instances_mut()?[index] = instance.into();
        self.instance_count += 1;

        Ok(())
    }

    pub fn extend(
        &mut self,
        context: &mut impl GraphicsContext<Backend = B>,
        instances: impl IntoIterator<Item = Instance>,
    ) -> Result<()> {
        let mut iter = instances.into_iter();
        loop {
            let inst_nb = self.tess.inst_nb();
            let mut instances = self.tess.instances_mut()?;
            for i in self.instance_count..inst_nb {
                instances[i] = match iter.next() {
                    Some(instance) => instance.into(),
                    None => return Ok(()),
                };
                self.instance_count += 1;
            }
            drop(instances);
            self.ensure_capacity(context, self.instance_count.next_power_of_two())?;
        }
    }

    pub fn extend_from_slice(
        &mut self,
        context: &mut impl GraphicsContext<Backend = B>,
        data: &[Instance],
    ) -> Result<()> {
        let new_count = self.instance_count + data.len();
        self.ensure_capacity(context, new_count)?;
        let mut instances = self.tess.instances_mut()?;
        instances[self.instance_count..new_count]
            .iter_mut()
            .zip(data)
            .for_each(|(dst, &src)| *dst = src.into());
        self.instance_count = new_count;
        Ok(())
    }

    pub fn instances(&mut self) -> Result<MeshInstances<B>, TessMapError> {
        Ok(MeshInstances {
            instances: self.tess.instances()?,
            instance_count: self.instance_count,
        })
    }

    pub fn instances_mut(&mut self) -> Result<MeshInstancesMut<B>, TessMapError> {
        Ok(MeshInstancesMut {
            instances_mut: self.tess.instances_mut()?,
            instance_count: self.instance_count,
        })
    }

    pub fn view(
        &self,
    ) -> Result<TessView<B, VertexData, u16, InstanceData, Interleaved>, TessViewError> {
        self.tess.inst_view(..self.index_count, self.instance_count)
    }
}

pub struct TryWriteInstances<'a, B: EvolBackend> {
    instances_mut: InstancesMut<'a, B, VertexData, u16, InstanceData, Interleaved, InstanceData>,
    instance_count: &'a mut usize,
}

impl<'a, B: EvolBackend> TryWriteInstances<'a, B> {
    #[inline]
    pub fn single_write(&mut self, instance: &Instance) -> Result<()> {
        ensure!(
            *self.instance_count < self.instances_mut.len(),
            "mesh instance buffer is full!"
        );
        self.instances_mut[*self.instance_count] = (*instance).into();
        *self.instance_count += 1;
        Ok(())
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        self.instances_mut.len() == *self.instance_count
    }
}

pub struct MeshInstances<'a, B: EvolBackend> {
    instances: Instances<'a, B, VertexData, u16, InstanceData, Interleaved, InstanceData>,
    instance_count: usize,
}

impl<'a, B: EvolBackend> Deref for MeshInstances<'a, B> {
    type Target = [InstanceData];

    fn deref(&self) -> &Self::Target {
        &self.instances[..self.instance_count]
    }
}

pub struct MeshInstancesMut<'a, B: EvolBackend> {
    instances_mut: InstancesMut<'a, B, VertexData, u16, InstanceData, Interleaved, InstanceData>,
    instance_count: usize,
}

impl<'a, B: EvolBackend> Deref for MeshInstancesMut<'a, B> {
    type Target = [InstanceData];

    fn deref(&self) -> &Self::Target {
        &self.instances_mut[..self.instance_count]
    }
}

impl<'a, B: EvolBackend> DerefMut for MeshInstancesMut<'a, B> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.instances_mut[..self.instance_count]
    }
}

#[derive(Debug)]
pub struct TransformStack {
    txs: Vec<Matrix4<f32>>,
}

impl Default for TransformStack {
    fn default() -> Self {
        Self::new()
    }
}

impl TransformStack {
    pub fn new() -> Self {
        Self {
            txs: vec![Matrix4::identity()],
        }
    }

    pub fn clear(&mut self) {
        self.txs.clear();
        self.txs.push(Matrix4::identity());
    }

    pub fn top(&self) -> &Matrix4<f32> {
        self.txs.last().unwrap()
    }

    pub fn top_mut(&mut self) -> &mut Matrix4<f32> {
        self.txs.last_mut().unwrap()
    }

    pub fn push(&mut self) {
        self.txs.push(*self.top());
    }

    pub fn pop(&mut self) {
        self.txs.pop();

        if self.txs.is_empty() {
            self.txs.push(Matrix4::identity());
        }
    }
}

impl Deref for TransformStack {
    type Target = Matrix4<f32>;

    fn deref(&self) -> &Self::Target {
        self.txs.last().unwrap()
    }
}

impl DerefMut for TransformStack {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.txs.last_mut().unwrap()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MeshId(Index);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextureId(Index);

#[derive(Debug, Clone)]
pub struct PooledText {
    text: String,
    scale: PxScale,
    font_id: FontId,
    extra: Extra,
}

#[derive(Debug, Clone)]
pub struct PooledSection {
    screen_position: Point2<f32>,
    bounds: Vector2<f32>,
    layout: Layout<BuiltInLineBreaker>,
    text: Vec<PooledText>,
}

impl PooledSection {
    fn to_section<'a>(&'a self, mut text: Vec<Text<'a>>) -> Section<'a> {
        text.extend(self.text.iter().map(|pooled_text| Text {
            text: pooled_text.text.as_str(),
            scale: pooled_text.scale,
            font_id: pooled_text.font_id,
            extra: pooled_text.extra,
        }));

        Section {
            screen_position: (self.screen_position.x, self.screen_position.y),
            bounds: (self.bounds.x, self.bounds.y),
            layout: self.layout,
            text,
        }
    }
}

pub struct EvolCommandBuffer {
    section_pool: Vec<Vec<Text<'static>>>,
    string_pool: Vec<String>,
    text_vec_pool: Vec<Vec<PooledText>>,
    #[allow(clippy::vec_box)]
    builder_pool: Vec<Box<MeshBuilder>>,
    matrices: Vec<Matrix4<f32>>,
    instances: Vec<Instance>,
    builders: Arena<Box<MeshBuilder>>,
    commands: Vec<EvolCommand>,
    line_thickness: f32,
    transforms: TransformStack,
    transforms_dirty: bool,
}

impl Default for EvolCommandBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl EvolCommandBuffer {
    pub fn new() -> Self {
        Self {
            section_pool: Vec::new(),
            string_pool: Vec::new(),
            text_vec_pool: Vec::new(),
            builder_pool: Vec::new(),
            matrices: Vec::new(),
            instances: Vec::new(),
            builders: Arena::new(),
            commands: Vec::new(),
            line_thickness: DEFAULT_LINE_THICKNESS,
            transforms: TransformStack::new(),
            transforms_dirty: true,
        }
    }

    pub fn clear(&mut self) {
        self.matrices.clear();
        self.instances.clear();
        self.commands.clear();
        self.transforms.clear();
        self.builder_pool
            .extend(self.builders.drain().map(|(_, b)| b));
        self.transforms_dirty = true;
    }

    #[inline(always)]
    fn clean_transforms(&mut self) {
        if self.transforms_dirty {
            let index = self.matrices.len();
            self.matrices.push(*self.transforms.top());
            self.commands.push(EvolCommand::SetModel(index));
        }
    }

    pub fn new_or_cached_section<'a>(&mut self) -> Section<'a> {
        self.section_pool
            .pop()
            .map(|text| Section {
                screen_position: (0., 0.),
                bounds: (f32::INFINITY, f32::INFINITY),
                layout: Layout::default(),
                text,
            })
            .unwrap_or_else(Section::new)
    }

    pub fn new_or_cached_mesh_builder(&mut self) -> Box<MeshBuilder> {
        self.builder_pool.pop().unwrap_or_default()
    }

    pub fn cache_section(&mut self, mut section: Section) {
        // Ensure that there is ***NO*** live data inside the `Section` which could somehow
        // mistakenly end up marked `'static` through this caching!!
        section.text.clear();
        self.section_pool
            .push(unsafe { std::mem::transmute(section.text) });
    }

    pub fn pool_text(&mut self, text: Text) -> PooledText {
        let mut string = self.string_pool.pop().unwrap_or_default();
        text.text.clone_into(&mut string);
        PooledText {
            text: string,
            scale: text.scale,
            font_id: text.font_id,
            extra: text.extra,
        }
    }

    pub fn pool_section(&mut self, mut section: Section) -> PooledSection {
        let mut text = self.text_vec_pool.pop().unwrap_or_default();
        for sub_text in section.text.drain(..) {
            text.push(self.pool_text(sub_text));
        }

        let (px, py) = section.screen_position;
        let (ex, ey) = section.bounds;
        let layout = section.layout;

        self.cache_section(section);

        PooledSection {
            screen_position: Point2::new(px, py),
            bounds: Vector2::new(ex, ey),
            layout,
            text,
        }
    }

    pub fn draw_text(&mut self, section: Section) -> Result<&mut Self> {
        self.clean_transforms();
        let pooled = self.pool_section(section);
        self.commands.push(EvolCommand::DrawText(pooled));
        Ok(self)
    }

    pub fn draw_textured_quad(
        &mut self,
        texture: TextureId,
        instance: &Instance,
    ) -> Result<&mut Self> {
        self.clean_transforms();
        let instance_id = self.instances.len();
        self.instances.push(*instance);
        self.commands
            .push(EvolCommand::DrawTexturedQuad(texture, instance_id));
        Ok(self)
    }

    pub fn draw_mesh(
        &mut self,
        texture: Option<TextureId>,
        mesh: MeshId,
        instance: &Instance,
    ) -> Result<&mut Self> {
        let instance_id = self.instances.len();
        self.instances.push(*instance);
        self.commands
            .push(EvolCommand::DrawMesh(texture, mesh, instance_id));
        Ok(self)
    }

    pub fn draw_mesh_instanced(
        &mut self,
        texture: Option<TextureId>,
        mesh: MeshId,
    ) -> Result<&mut Self> {
        self.clean_transforms();
        self.commands
            .push(EvolCommand::DrawMeshInstanced(texture, mesh));
        Ok(self)
    }

    pub fn draw_wireframe(
        &mut self,
        texture: Option<TextureId>,
        mesh: MeshId,
        instance: &Instance,
    ) -> Result<&mut Self> {
        let instance_id = self.instances.len();
        self.instances.push(*instance);
        self.commands
            .push(EvolCommand::DrawWireframe(texture, mesh, instance_id));
        Ok(self)
    }

    pub fn draw_wireframe_instanced(
        &mut self,
        texture: Option<TextureId>,
        mesh: MeshId,
    ) -> Result<&mut Self> {
        self.clean_transforms();
        self.commands
            .push(EvolCommand::DrawWireframeInstanced(texture, mesh));
        Ok(self)
    }

    pub fn draw_transient_mesh(
        &mut self,
        texture: Option<TextureId>,
        &mesh_params: &MeshBuilderParams,
        &instance: &Instance,
    ) -> &mut Self {
        let instance_id = self.instances.len();
        self.instances.push(instance);
        self.commands.push(EvolCommand::DrawTransientMesh(
            texture,
            mesh_params,
            instance_id,
        ));
        self
    }

    pub fn draw_transient_wireframe(
        &mut self,
        texture: Option<TextureId>,
        &mesh_params: &MeshBuilderParams,
        &instance: &Instance,
    ) -> &mut Self {
        let instance_id = self.instances.len();
        self.instances.push(instance);
        self.commands.push(EvolCommand::DrawTransientWireframe(
            texture,
            mesh_params,
            instance_id,
        ));
        self
    }

    pub fn draw_streamed_mesh(
        &mut self,
        texture: Option<TextureId>,
        mesh_builder: Box<MeshBuilder>,
        instance: &Instance,
    ) -> &mut Self {
        let instance_id = self.instances.len();
        self.instances.push(*instance);
        let builder_id = self.builders.insert(mesh_builder);
        self.commands.push(EvolCommand::DrawStreamedMesh(
            texture,
            builder_id,
            instance_id,
        ));
        self
    }

    pub fn draw_streamed_wireframe(
        &mut self,
        texture: Option<TextureId>,
        mesh_builder: Box<MeshBuilder>,
        instance: &Instance,
    ) -> &mut Self {
        let instance_id = self.instances.len();
        self.instances.push(*instance);
        let builder_id = self.builders.insert(mesh_builder);
        self.commands.push(EvolCommand::DrawStreamedWireframe(
            texture,
            builder_id,
            instance_id,
        ));
        self
    }

    #[inline(always)]
    fn dirty_transforms(&mut self) -> &mut TransformStack {
        self.transforms_dirty = true;
        &mut self.transforms
    }

    pub fn apply_transform(&mut self, tx: &Matrix4<f32>) -> &mut Self {
        **self.dirty_transforms() *= tx;
        self
    }

    pub fn inverse_transform_point(&self, pt: &Point3<f32>) -> Point3<f32> {
        self.transforms
            .try_inverse()
            .expect("non-invertible transform!")
            .transform_point(pt)
    }

    pub fn origin(&mut self) -> &mut Self {
        **self.dirty_transforms() = Matrix4::identity();
        self
    }

    pub fn push(&mut self) -> &mut Self {
        self.dirty_transforms().push();
        self
    }

    pub fn pop(&mut self) -> &mut Self {
        self.dirty_transforms().pop();
        self
    }

    pub fn replace_transform(&mut self, tx: &Matrix4<f32>) -> &mut Self {
        **self.dirty_transforms() = *tx;
        self
    }

    pub fn rotate(&mut self, axis: &UnitVector3<f32>, angle: f32) -> &mut Self {
        **self.dirty_transforms() *=
            UnitQuaternion::new(axis.into_inner() * angle).to_homogeneous();
        self
    }

    /// The scaling vector must be all nonzero.
    pub fn scale(&mut self, scale: &Vector3<f32>) -> Result<&mut Self> {
        **self.dirty_transforms() *= Matrix4::new_nonuniform_scaling(scale);
        Ok(self)
    }

    pub fn transform_point(&self, pt: &Point3<f32>) -> Point3<f32> {
        self.transforms.transform_point(pt)
    }

    pub fn translate(&mut self, v: &Vector3<f32>) -> &mut Self {
        **self.dirty_transforms() *= Matrix4::new_translation(v);
        self
    }

    pub fn set_line_thickness(&mut self, thickness: f32) -> &mut Self {
        if self.line_thickness != thickness {
            self.commands.push(EvolCommand::SetLineThickness(thickness));
        }
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MeshBuilderParams {
    UvSphere {
        u: u32,
        v: u32,
        radius: Total<f32>,
        color: [Total<f32>; 4],
    },
    UvHemisphere {
        u: u32,
        v: u32,
        radius: Total<f32>,
        color: [Total<f32>; 4],
    },
    Cylinder {
        u: u32,
        half_height: Total<f32>,
        radius: Total<f32>,
        color: [Total<f32>; 4],
    },
    UvCapsule {
        u: u32,
        v: u32,
        half_height: Total<f32>,
        radius: Total<f32>,
        color: [Total<f32>; 4],
    },
}

impl MeshBuilderParams {
    pub fn uv_sphere(u: u32, v: u32, radius: f32, color: Color) -> Self {
        Self::UvSphere {
            u,
            v,
            radius: Total::from_inner(radius),
            color: <[f32; 4]>::from(color).map(Total::from_inner),
        }
    }

    pub fn uv_hemisphere(u: u32, v: u32, radius: f32, color: Color) -> Self {
        Self::UvHemisphere {
            u,
            v,
            radius: Total::from_inner(radius),
            color: <[f32; 4]>::from(color).map(Total::from_inner),
        }
    }

    pub fn cylinder(u: u32, half_height: f32, radius: f32, color: Color) -> Self {
        Self::Cylinder {
            u,
            half_height: Total::from_inner(half_height),
            radius: Total::from_inner(radius),
            color: <[f32; 4]>::from(color).map(Total::from_inner),
        }
    }

    pub fn uv_capsule(u: u32, v: u32, half_height: f32, radius: f32, color: Color) -> Self {
        Self::UvCapsule {
            u,
            v,
            half_height: Total::from_inner(half_height),
            radius: Total::from_inner(radius),
            color: <[f32; 4]>::from(color).map(Total::from_inner),
        }
    }
}

pub struct CachedMesh<B: EvolBackend> {
    pub counter: u64,
    pub mesh: Mesh<B>,
}

pub struct MeshCache<B: EvolBackend> {
    map: HashMap<MeshBuilderParams, CachedMesh<B>>,
}

impl<B: EvolBackend> Default for MeshCache<B> {
    fn default() -> Self {
        Self::new()
    }
}

impl<B: EvolBackend> MeshCache<B> {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn get_or_insert<'a>(
        &'a mut self,
        builder: &mut MeshBuilder,
        params: MeshBuilderParams,
        context: &mut impl GraphicsContext<Backend = B>,
    ) -> Result<&'a mut CachedMesh<B>> {
        use std::collections::hash_map::Entry;
        match self.map.entry(params) {
            Entry::Occupied(occupied) => Ok(occupied.into_mut()),
            Entry::Vacant(vacant) => {
                builder.reset();
                let mesh = match params {
                    MeshBuilderParams::UvSphere {
                        u,
                        v,
                        radius,
                        color,
                    } => builder.uv_sphere(
                        u,
                        v,
                        radius.into_inner(),
                        &Color::from(color.map(Total::into_inner)),
                    )?,
                    MeshBuilderParams::UvHemisphere {
                        u,
                        v,
                        radius,
                        color,
                    } => builder.uv_hemisphere(
                        u,
                        v,
                        radius.into_inner(),
                        &Color::from(color.map(Total::into_inner)),
                    )?,
                    MeshBuilderParams::Cylinder {
                        u,
                        half_height,
                        radius,
                        color,
                    } => builder.cylinder(
                        u,
                        half_height.into_inner(),
                        radius.into_inner(),
                        &Color::from(color.map(Total::into_inner)),
                    )?,
                    MeshBuilderParams::UvCapsule {
                        u,
                        v,
                        half_height,
                        radius,
                        color,
                    } => builder.uv_capsule(
                        u,
                        v,
                        half_height.into_inner(),
                        radius.into_inner(),
                        &Color::from(color.map(Total::into_inner)),
                    )?,
                }
                .build(context, 1024)?;

                Ok(vacant.insert(CachedMesh { counter: 0, mesh }))
            }
        }
    }
}

#[derive(Debug)]
enum EvolCommand {
    SetModel(usize),
    DrawText(PooledSection),
    DrawTexturedQuad(TextureId, usize),
    DrawMesh(Option<TextureId>, MeshId, usize),
    DrawMeshInstanced(Option<TextureId>, MeshId),
    DrawTransientMesh(Option<TextureId>, MeshBuilderParams, usize),
    DrawStreamedMesh(Option<TextureId>, Index, usize),
    SetLineThickness(f32),
    DrawWireframe(Option<TextureId>, MeshId, usize),
    DrawWireframeInstanced(Option<TextureId>, MeshId),
    DrawTransientWireframe(Option<TextureId>, MeshBuilderParams, usize),
    DrawStreamedWireframe(Option<TextureId>, Index, usize),
}

pub struct EvolRenderer<B: EvolBackend> {
    fill_program: Program<B, VertexSemantics, (), Uniforms>,
    wireframe_program: Program<B, VertexSemantics, (), Uniforms>,
    meshes: Arena<Mesh<B>>,
    textures: Arena<Texture<B, Dim2, NormRGBA8UI>>,
    mesh_cache: MeshCache<B>,
    mesh_builder: MeshBuilder,
    glyph_brush: GlyphBrush<B>,
    white: TextureId,
    quad: MeshId,
    streaming_mesh: Mesh<B>,
}

impl<B: EvolBackend> EvolRenderer<B> {
    pub fn new(context: &mut impl GraphicsContext<Backend = B>) -> Result<Self> {
        let font =
            FontArc::try_from_slice(include_bytes!("../../resources/Inconsolata-Regular.ttf"))
                .expect("should be valid!");
        let glyph_brush = GlyphBrushBuilder::using_font(font).build(context);
        let mut meshes = Arena::new();
        let mut textures = Arena::new();
        let white = {
            let texture = context.new_texture(
                [1, 1],
                Sampler::default(),
                TexelUpload::base_level(&[[255, 255, 255, 255]], 0),
            )?;
            TextureId(textures.insert(texture))
        };

        let mut mesh_builder = MeshBuilder::new();
        let quad = MeshId(
            meshes.insert(
                mesh_builder
                    .quad(&Point2::origin(), &Vector2::repeat(1.))?
                    .build(context, 1)?,
            ),
        );

        let streaming_mesh = Mesh {
            tess: context
                .new_tess()
                .set_mode(Mode::Triangle)
                .set_vertices(vec![VertexData::default(); 1024])
                .set_indices(vec![0; 1024])
                .set_instances(vec![InstanceData::default()])
                .build()?,
            index_count: 0,
            instance_count: 1,
        };

        let built_fill_program = context.new_shader_program().from_strings(
            include_str!("evol/evol_es300.glslv"),
            None,
            None,
            include_str!("evol/evol_es300.glslf"),
        )?;

        let built_wireframe_program = context.new_shader_program().from_strings(
            include_str!("evol/evol_es300.glslv"),
            None,
            None,
            include_str!("evol/evol_es300_wireframe.glslf"),
        )?;

        // FIXME(shea): log warnings
        // FIXME(maxim): logging

        let fill_program = built_fill_program.ignore_warnings();
        let wireframe_program = built_wireframe_program.ignore_warnings();

        Ok(Self {
            fill_program,
            wireframe_program,
            meshes,
            textures,
            mesh_cache: MeshCache::new(),
            mesh_builder: MeshBuilder::new(),
            glyph_brush,
            white,
            quad,
            streaming_mesh,
        })
    }

    pub fn insert_mesh(&mut self, mesh: Mesh<B>) -> MeshId {
        MeshId(self.meshes.insert(mesh))
    }

    pub fn insert_texture(&mut self, texture: Texture<B, Dim2, NormRGBA8UI>) -> TextureId {
        TextureId(self.textures.insert(texture))
    }

    pub fn draw_buffered<CS, DS>(
        &mut self,
        context: &mut impl GraphicsContext<Backend = B>,
        pipeline_state: &PipelineState,
        framebuffer: &Framebuffer<B, Dim2, CS, DS>,
        buffer: &mut EvolCommandBuffer,
        view_projection: &Matrix4<f32>,
    ) -> Result<()>
    where
        CS: ColorSlot<B, Dim2>,
        DS: DepthStencilSlot<B, Dim2>,
    {
        let mut commands = buffer.commands.drain(..).peekable();
        let mut model = Matrix4::identity();
        let y_flip = Matrix4::new_nonuniform_scaling(&Vector3::new(1., -1., 1.));
        let render_state = RenderState::default().set_blending(Blending {
            equation: Equation::Additive,
            src: Factor::SrcAlpha,
            dst: Factor::SrcAlphaComplement,
        });
        let mut line_thickness = DEFAULT_LINE_THICKNESS;

        while let Some(command) = commands.peek() {
            match command {
                EvolCommand::SetModel(model_id) => {
                    model = buffer.matrices[*model_id];
                    commands.next();
                }
                EvolCommand::SetLineThickness(thickness) => {
                    line_thickness = *thickness;
                    commands.next();
                }
                EvolCommand::DrawText(_) => {
                    while let Some(command) = commands.peek() {
                        match command {
                            EvolCommand::DrawText(pooled_text) => {
                                let text_buf = buffer.section_pool.pop().unwrap_or_default();
                                let mut section = pooled_text.to_section(text_buf);

                                // glyph-brush uses a top-left origin with a left-handed coordinate
                                // system; to get around this, we flip all of our Y coordinates and
                                // Z coordinates, giving us a bottom-left origin w/ right-handed
                                // coordinates.
                                section.screen_position.1 = -section.screen_position.1;
                                for text in section.text.iter_mut() {
                                    text.extra.z = -text.extra.z;
                                }

                                self.glyph_brush.queue(section);
                                commands.next();
                            }
                            _ => break,
                        }
                    }

                    self.glyph_brush.process_queued(context);

                    context
                        .new_pipeline_gate()
                        .pipeline(
                            framebuffer,
                            pipeline_state,
                            |mut pipeline, mut shading_gate| {
                                let mvp = y_flip * view_projection * model;
                                self.glyph_brush.draw_queued_with_transform(
                                    &mut pipeline,
                                    &mut shading_gate,
                                    *mvp.as_slice().split_array_ref().0,
                                )
                            },
                        )
                        .into_result()?;
                }
                &EvolCommand::DrawTexturedQuad(texture_id, instance) => {
                    let do_draw = |pipeline: Pipeline<B>, mut shading_gate: ShadingGate<B>| {
                        let first_texture_id = texture_id;

                        let quad_mesh = &mut self.meshes[self.quad.0];
                        quad_mesh.clear();
                        quad_mesh.try_push(&buffer.instances[instance])?;

                        commands.next();

                        let mut writer = quad_mesh.try_write()?;
                        while let Some(EvolCommand::DrawTexturedQuad(texture_id, instance_id)) =
                            commands.peek()
                        {
                            if *texture_id != first_texture_id || writer.is_full() {
                                break;
                            }
                            writer.single_write(&buffer.instances[*instance_id])?;
                            commands.next();
                        }
                        drop(writer);

                        let bound = pipeline
                            .bind_texture::<Dim2, NormRGBA8UI>(
                                &mut self.textures[first_texture_id.0],
                            )?
                            .binding();

                        let quad_mesh = &mut self.meshes[self.quad.0];
                        quad_mesh.clear();
                        quad_mesh.try_push(&buffer.instances[instance])?;
                        commands.next();
                        let mut writer = quad_mesh.try_write()?;
                        while let Some(EvolCommand::DrawTexturedQuad(texture_id, instance_id)) =
                            commands.peek()
                        {
                            if *texture_id != first_texture_id || writer.is_full() {
                                break;
                            }
                            writer.single_write(&buffer.instances[*instance_id])?;
                            commands.next();
                        }
                        drop(writer);

                        shading_gate.shade(
                            &mut self.fill_program,
                            |mut iface, uni, mut render_gate| {
                                iface.set(&uni.texture, bound);
                                iface.set(
                                    &uni.view_projection,
                                    Mat44::from((view_projection * model).data.0),
                                );

                                render_gate.render(&render_state, |mut tess_gate| {
                                    quad_mesh.view().and_then(|view| tess_gate.render(view))
                                })
                            },
                        )?;

                        Ok::<_, Error>(())
                    };

                    context
                        .new_pipeline_gate()
                        .pipeline(framebuffer, pipeline_state, do_draw)
                        .into_result()?;
                }
                &EvolCommand::DrawMesh(texture_id, mesh_id, instance_id) => {
                    let do_draw = |pipeline: Pipeline<B>, mut shading_gate: ShadingGate<B>| {
                        let first_texture_id = texture_id;
                        let first_mesh_id = mesh_id;
                        let resolved_texture_id = first_texture_id.unwrap_or(self.white);
                        let bound = pipeline.bind_texture::<Dim2, NormRGBA8UI>(
                            &mut self.textures[resolved_texture_id.0],
                        )?;

                        let mesh = &mut self.meshes[first_mesh_id.0];
                        mesh.clear();
                        mesh.try_push(&buffer.instances[instance_id])?;
                        commands.next();
                        let mut writer = mesh.try_write()?;
                        while let Some(EvolCommand::DrawMesh(texture_id, mesh_id, instance_id)) =
                            commands.peek()
                        {
                            if *texture_id != first_texture_id
                                || *mesh_id != first_mesh_id
                                || writer.is_full()
                            {
                                break;
                            }
                            writer.single_write(&buffer.instances[*instance_id])?;
                            commands.next();
                        }
                        drop(writer);

                        shading_gate.shade(
                            &mut self.fill_program,
                            |mut iface, uni, mut render_gate| {
                                iface.set(&uni.texture, bound.binding());
                                iface.set(
                                    &uni.view_projection,
                                    Mat44::from((view_projection * model).data.0),
                                );

                                render_gate.render(&render_state, |mut tess_gate| {
                                    mesh.view().and_then(|view| tess_gate.render(view))
                                })
                            },
                        )?;

                        Ok::<_, Error>(())
                    };

                    context
                        .new_pipeline_gate()
                        .pipeline(framebuffer, pipeline_state, do_draw)
                        .into_result()?;
                }
                &EvolCommand::DrawTransientMesh(texture_id, mesh_params, instance_id) => {
                    let first_mesh_params = mesh_params;
                    let cached = self.mesh_cache.get_or_insert(
                        &mut self.mesh_builder,
                        mesh_params,
                        context,
                    )?;

                    let do_draw = |pipeline: Pipeline<B>, mut shading_gate: ShadingGate<B>| {
                        let first_texture_id = texture_id;
                        let resolved_texture_id = first_texture_id.unwrap_or(self.white);
                        let bound = pipeline.bind_texture::<Dim2, NormRGBA8UI>(
                            &mut self.textures[resolved_texture_id.0],
                        )?;

                        cached.mesh.clear();
                        cached.mesh.try_push(&buffer.instances[instance_id])?;
                        commands.next();
                        let mut writer = cached.mesh.try_write()?;
                        while let Some(EvolCommand::DrawTransientMesh(
                            texture_id,
                            mesh_params,
                            instance_id,
                        )) = commands.peek()
                        {
                            if *texture_id != first_texture_id
                                || *mesh_params != first_mesh_params
                                || writer.is_full()
                            {
                                break;
                            }
                            writer.single_write(&buffer.instances[*instance_id])?;
                            commands.next();
                        }
                        drop(writer);

                        shading_gate.shade(
                            &mut self.fill_program,
                            |mut iface, uni, mut render_gate| {
                                iface.set(&uni.texture, bound.binding());
                                iface.set(
                                    &uni.view_projection,
                                    Mat44::from((view_projection * model).data.0),
                                );

                                render_gate.render(&render_state, |mut tess_gate| {
                                    cached.mesh.view().and_then(|view| tess_gate.render(view))
                                })
                            },
                        )?;

                        Ok::<_, Error>(())
                    };

                    context
                        .new_pipeline_gate()
                        .pipeline(framebuffer, pipeline_state, do_draw)
                        .into_result()?;
                }
                EvolCommand::DrawMeshInstanced(maybe_texture_id, mesh_id) => {
                    let do_draw = |pipeline: Pipeline<B>, mut shading_gate: ShadingGate<B>| {
                        let maybe_texture_id = *maybe_texture_id;
                        let texture_id = maybe_texture_id.unwrap_or(self.white);
                        let bound = pipeline
                            .bind_texture::<Dim2, NormRGBA8UI>(&mut self.textures[texture_id.0])?
                            .binding();
                        let mesh = &self.meshes[mesh_id.0];

                        shading_gate.shade(
                            &mut self.fill_program,
                            |mut iface, uni, mut render_gate| {
                                iface.set(&uni.texture, bound);
                                iface.set(
                                    &uni.view_projection,
                                    Mat44::from((view_projection * model).data.0),
                                );

                                render_gate.render(&render_state, |mut tess_gate| {
                                    mesh.view().and_then(|view| tess_gate.render(view))
                                })
                            },
                        )?;

                        Ok::<_, Error>(())
                    };

                    context
                        .new_pipeline_gate()
                        .pipeline(framebuffer, pipeline_state, do_draw)
                        .into_result()?;

                    commands.next();
                }
                &EvolCommand::DrawStreamedMesh(texture_id, builder_id, instance_id) => {
                    let mut builder = buffer.builders.remove(builder_id).unwrap();
                    builder.build_into(context, &mut self.streaming_mesh, 1)?;
                    builder.reset();
                    buffer.builder_pool.push(builder);

                    commands.next();

                    let do_draw = |pipeline: Pipeline<B>, mut shading_gate: ShadingGate<B>| {
                        let texture_id = texture_id.unwrap_or(self.white);
                        let bound = pipeline
                            .bind_texture::<Dim2, NormRGBA8UI>(&mut self.textures[texture_id.0])?
                            .binding();

                        self.streaming_mesh.clear();
                        self.streaming_mesh
                            .try_push(&buffer.instances[instance_id])?;

                        shading_gate.shade(
                            &mut self.fill_program,
                            |mut iface, uni, mut render_gate| {
                                iface.set(&uni.texture, bound);
                                iface.set(
                                    &uni.view_projection,
                                    Mat44::from((view_projection * model).data.0),
                                );

                                render_gate.render(&render_state, |mut tess_gate| {
                                    self.streaming_mesh
                                        .view()
                                        .and_then(|view| tess_gate.render(view))
                                })
                            },
                        )?;

                        Ok::<_, Error>(())
                    };

                    context
                        .new_pipeline_gate()
                        .pipeline(framebuffer, pipeline_state, do_draw)
                        .into_result()?;
                }
                &EvolCommand::DrawWireframe(texture_id, mesh_id, instance_id) => {
                    let do_draw = |pipeline: Pipeline<B>, mut shading_gate: ShadingGate<B>| {
                        let first_texture_id = texture_id;
                        let first_mesh_id = mesh_id;
                        let resolved_texture_id = first_texture_id.unwrap_or(self.white);
                        let bound = pipeline
                            .bind_texture::<Dim2, NormRGBA8UI>(
                                &mut self.textures[resolved_texture_id.0],
                            )?
                            .binding();

                        let mesh = &mut self.meshes[first_mesh_id.0];
                        mesh.clear();
                        mesh.try_push(&buffer.instances[instance_id])?;
                        commands.next();
                        let mut writer = mesh.try_write()?;
                        while let Some(EvolCommand::DrawWireframe(
                            texture_id,
                            mesh_id,
                            instance_id,
                        )) = commands.peek()
                        {
                            if *texture_id != first_texture_id
                                || *mesh_id != first_mesh_id
                                || writer.is_full()
                            {
                                break;
                            }
                            writer.single_write(&buffer.instances[*instance_id])?;
                            commands.next();
                        }
                        drop(writer);

                        shading_gate.shade(
                            &mut self.wireframe_program,
                            |mut iface, uni, mut render_gate| {
                                iface.set(&uni.texture, bound);
                                iface.set(&uni.line_thickness, line_thickness);
                                iface.set(
                                    &uni.view_projection,
                                    Mat44::from((view_projection * model).data.0),
                                );

                                render_gate.render(&render_state, |mut tess_gate| {
                                    mesh.view().and_then(|view| tess_gate.render(view))
                                })
                            },
                        )?;

                        Ok::<_, Error>(())
                    };

                    context
                        .new_pipeline_gate()
                        .pipeline(framebuffer, pipeline_state, do_draw)
                        .into_result()?;
                }
                EvolCommand::DrawWireframeInstanced(maybe_texture_id, mesh_id) => {
                    let do_draw = |pipeline: Pipeline<B>, mut shading_gate: ShadingGate<B>| {
                        let maybe_texture_id = *maybe_texture_id;
                        let texture_id = maybe_texture_id.unwrap_or(self.white);
                        let bound = pipeline
                            .bind_texture::<Dim2, NormRGBA8UI>(&mut self.textures[texture_id.0])?
                            .binding();
                        let mesh = &self.meshes[mesh_id.0];

                        shading_gate.shade(
                            &mut self.wireframe_program,
                            |mut iface, uni, mut render_gate| {
                                iface.set(&uni.texture, bound);
                                iface.set(&uni.line_thickness, line_thickness);
                                iface.set(
                                    &uni.view_projection,
                                    Mat44::from((view_projection * model).data.0),
                                );

                                render_gate.render(&render_state, |mut tess_gate| {
                                    mesh.view().and_then(|view| tess_gate.render(view))
                                })
                            },
                        )?;

                        Ok::<_, Error>(())
                    };

                    context
                        .new_pipeline_gate()
                        .pipeline(framebuffer, pipeline_state, do_draw)
                        .into_result()?;

                    commands.next();
                }
                &EvolCommand::DrawTransientWireframe(texture_id, mesh_params, instance_id) => {
                    let first_mesh_params = mesh_params;
                    let cached = self.mesh_cache.get_or_insert(
                        &mut self.mesh_builder,
                        mesh_params,
                        context,
                    )?;

                    let do_draw = |pipeline: Pipeline<B>, mut shading_gate: ShadingGate<B>| {
                        let first_texture_id = texture_id;
                        let resolved_texture_id = first_texture_id.unwrap_or(self.white);
                        let bound = pipeline.bind_texture::<Dim2, NormRGBA8UI>(
                            &mut self.textures[resolved_texture_id.0],
                        )?;

                        cached.mesh.clear();
                        cached.mesh.try_push(&buffer.instances[instance_id])?;
                        commands.next();
                        let mut writer = cached.mesh.try_write()?;
                        while let Some(EvolCommand::DrawTransientMesh(
                            texture_id,
                            mesh_params,
                            instance_id,
                        )) = commands.peek()
                        {
                            if *texture_id != first_texture_id
                                || *mesh_params != first_mesh_params
                                || writer.is_full()
                            {
                                break;
                            }
                            writer.single_write(&buffer.instances[*instance_id])?;
                            commands.next();
                        }
                        drop(writer);

                        shading_gate.shade(
                            &mut self.wireframe_program,
                            |mut iface, uni, mut render_gate| {
                                iface.set(&uni.texture, bound.binding());
                                iface.set(&uni.line_thickness, line_thickness);
                                iface.set(
                                    &uni.view_projection,
                                    Mat44::from((view_projection * model).data.0),
                                );

                                render_gate.render(&render_state, |mut tess_gate| {
                                    cached.mesh.view().and_then(|view| tess_gate.render(view))
                                })
                            },
                        )?;

                        Ok::<_, Error>(())
                    };

                    context
                        .new_pipeline_gate()
                        .pipeline(framebuffer, pipeline_state, do_draw)
                        .into_result()?;
                }
                &EvolCommand::DrawStreamedWireframe(texture_id, builder_id, instance_id) => {
                    let mut builder = buffer.builders.remove(builder_id).unwrap();
                    builder.build_into(context, &mut self.streaming_mesh, 1)?;
                    builder.reset();
                    buffer.builder_pool.push(builder);

                    commands.next();

                    let do_draw = |pipeline: Pipeline<B>, mut shading_gate: ShadingGate<B>| {
                        let texture_id = texture_id.unwrap_or(self.white);
                        let bound = pipeline
                            .bind_texture::<Dim2, NormRGBA8UI>(&mut self.textures[texture_id.0])?
                            .binding();

                        self.streaming_mesh.clear();
                        self.streaming_mesh
                            .try_push(&buffer.instances[instance_id])?;

                        shading_gate.shade(
                            &mut self.wireframe_program,
                            |mut iface, uni, mut render_gate| {
                                iface.set(&uni.texture, bound);
                                iface.set(&uni.line_thickness, line_thickness);
                                iface.set(
                                    &uni.view_projection,
                                    Mat44::from((view_projection * model).data.0),
                                );

                                render_gate.render(&render_state, |mut tess_gate| {
                                    self.streaming_mesh
                                        .view()
                                        .and_then(|view| tess_gate.render(view))
                                })
                            },
                        )?;

                        Ok::<_, Error>(())
                    };

                    context
                        .new_pipeline_gate()
                        .pipeline(framebuffer, pipeline_state, do_draw)
                        .into_result()?;
                }
            }
        }

        drop(commands);
        buffer.clear();

        Ok(())
    }
}
