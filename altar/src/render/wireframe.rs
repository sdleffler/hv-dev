use crate::render::{
    mesh::{HasNormal, HasPosition},
    pipeline::semantics::*,
    Color, LinearColor, Transform,
};
use hv::{
    ecs::{Or, PreparedQuery, SystemContext},
    prelude::*,
};
use luminance::{
    backend::{
        framebuffer::Framebuffer as FramebufferBackend,
        pipeline::{Pipeline as PipelineBackend, PipelineShaderData},
        render_gate::RenderGate as RenderGateBackend,
        shader::{Shader as ShaderBackend, ShaderData as ShaderDataBackend, Uniformable},
        tess::{InstanceSlice as InstanceSliceBackend, Tess as TessBackend},
        tess_gate::TessGate as TessGateBackend,
    },
    context::GraphicsContext,
    face_culling::{FaceCulling, FaceCullingMode, FaceCullingOrder},
    pipeline::{Pipeline, PipelineError, ShaderDataBinding},
    render_state::RenderState,
    shader::{
        types::{Mat44, Vec2, Vec3, Vec4},
        Program, ShaderData, Uniform,
    },
    shading_gate::ShadingGate,
    tess::{Interleaved, Mode, Tess, TessBuilder, TessView},
    texture::Dim2,
    UniformInterface, Vertex,
};
use std::ops;
use thunderdome::{Arena, Index};

#[derive(Clone, Copy, Debug, Vertex, PartialEq)]
#[vertex(sem = "VertexSemantics")]
pub struct Vertex {
    pub position: VertexPosition,
    #[vertex(normalized = true)]
    pub color: VertexColor,
    pub normal: VertexNormal,
}

impl HasPosition<Vector3<f32>> for Vertex {
    fn get_position(&self) -> Vector3<f32> {
        self.position.into()
    }

    fn set_position(&mut self, position: Vector3<f32>) {
        self.position = position.into();
    }
}

impl HasNormal for Vertex {
    fn get_normal(&self) -> Vector3<f32> {
        self.normal.into()
    }

    fn set_normal(&mut self, normal: Vector3<f32>) {
        self.normal = normal.into();
    }
}

impl From<Vector3<f32>> for Vertex {
    fn from(v: Vector3<f32>) -> Self {
        Self {
            position: v.into(),
            color: LinearColor::WHITE.into(),
            normal: Vector3::zeros().into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Vertex)]
#[vertex(sem = "VertexSemantics", instanced = "true")]
pub struct Instance {
    #[vertex(normalized = true)]
    pub color: InstanceColor,
}

impl Default for Instance {
    fn default() -> Self {
        Self {
            color: InstanceColor::new(LinearColor::WHITE.into()),
        }
    }
}

#[derive(Debug, UniformInterface)]
pub struct Uniforms {
    #[uniform(unbound, name = "u_Tx")]
    pub tx: Uniform<Mat44<f32>>,
    #[uniform(unbound, name = "u_View")]
    pub view: Uniform<Mat44<f32>>,
    #[uniform(unbound, name = "u_MVP")]
    pub mvp: Uniform<Mat44<f32>>,
    #[uniform(unbound, name = "u_Color")]
    pub color: Uniform<Vec4<f32>>,
    #[uniform(unbound, name = "u_FogDistance")]
    pub fog_distance: Uniform<f32>,
    #[uniform(unbound, name = "u_LightDirection")]
    pub light_direction: Uniform<Vec3<f32>>,
    #[uniform(unbound, name = "u_LightDiffuseColor")]
    pub light_diffuse_color: Uniform<Vec3<f32>>,
    #[uniform(unbound, name = "u_LightBackColor")]
    pub light_back_color: Uniform<Vec3<f32>>,
    #[uniform(unbound, name = "u_LightAmbientColor")]
    pub light_ambient_color: Uniform<Vec3<f32>>,
    #[uniform(unbound, name = "u_InstanceTxs")]
    pub instance_txs: Uniform<ShaderDataBinding<Mat44<f32>>>,
    #[uniform(unbound, name = "u_Thickness")]
    pub thickness: Uniform<f32>,
    #[uniform(unbound, name = "u_Resolution")]
    pub resolution: Uniform<Vec2<f32>>,
    #[uniform(unbound, name = "u_Positions")]
    pub positions: Uniform<ShaderDataBinding<Vec4<f32>>>,
    #[uniform(unbound, name = "u_Colors")]
    pub colors: Uniform<ShaderDataBinding<Vec4<f32>>>,
    #[uniform(unbound, name = "u_Indices")]
    pub indices: Uniform<ShaderDataBinding<u32>>,
}

pub const STATIC_VERTEX_SRC: &str = include_str!("wireframe/static_wireframe_es300.glslv");
pub const DYNAMIC_VERTEX_SRC: &str = include_str!("wireframe/dynamic_wireframe_es300.glslv");
pub const FRAGMENT_SRC: &str = include_str!("wireframe/wireframe_es300.glslf");
pub const LINE_VERTEX_SRC: &str = include_str!("wireframe/line_wireframe_es300.glslv");
pub const LINE_FRAGMENT_SRC: &str = include_str!("wireframe/line_wireframe_es300.glslf");

pub struct StaticWireframeTess<B: ?Sized>
where
    B: WireframeBackend,
{
    tess: Tess<B, Vertex, u16, (), Interleaved>,
    enable_lighting: bool,
}

impl<B> StaticWireframeTess<B>
where
    B: WireframeBackend,
{
    pub fn tess(&self) -> &Tess<B, Vertex, u16, (), Interleaved> {
        &self.tess
    }

    pub fn tess_mut(&mut self) -> &mut Tess<B, Vertex, u16, (), Interleaved> {
        &mut self.tess
    }

    pub fn is_lighting_enabled(&self) -> bool {
        self.enable_lighting
    }

    pub fn set_lighting_enabled(&mut self, enabled: bool) {
        self.enable_lighting = enabled;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WireframeInstance {
    /// The color of this instance.
    pub color: LinearColor,
    /// The transform of this instance.
    pub tx: Matrix4<f32>,
}

pub struct DynamicWireframeTess<B: ?Sized>
where
    B: WireframeBackend,
{
    tess: Tess<B, Vertex, u16, Instance, Interleaved>,
    enable_lighting: bool,
    instance_list: Vec<WireframeInstance>,
}

impl<B> DynamicWireframeTess<B>
where
    B: WireframeBackend,
{
    pub fn tess(&self) -> &Tess<B, Vertex, u16, Instance, Interleaved> {
        &self.tess
    }

    pub fn tess_mut(&mut self) -> &mut Tess<B, Vertex, u16, Instance, Interleaved> {
        &mut self.tess
    }

    pub fn is_lighting_enabled(&self) -> bool {
        self.enable_lighting
    }

    pub fn set_lighting_enabled(&mut self, enabled: bool) {
        self.enable_lighting = enabled;
    }

    pub fn queue_instance(&mut self, instance: WireframeInstance) {
        self.instance_list.push(instance);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StaticWireframeTessId(Index);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DynamicWireframeTessId(Index);

/// Component
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StaticWireframe {
    pub tess_id: StaticWireframeTessId,
}

/// Component
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DynamicWireframe {
    pub tess_id: DynamicWireframeTessId,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct LineVertex {
    pub pos: Vector3<f32>,
    pub color: Color,
}

#[derive(Debug, Clone, Copy)]
struct LineCommand {
    start: usize,
    len: usize,
    cap: bool,
}

pub trait WireframeBackend:
    TessBackend<Vertex, u16, (), Interleaved>
    + TessBackend<Vertex, u16, Instance, Interleaved>
    + TessBackend<(), u16, (), Interleaved>
    + ShaderBackend
    + PipelineBackend<Dim2>
    + FramebufferBackend<Dim2>
    + RenderGateBackend
    + TessGateBackend<Vertex, u16, (), Interleaved>
    + TessGateBackend<Vertex, u16, Instance, Interleaved>
    + TessGateBackend<(), u16, (), Interleaved>
    + ShaderDataBackend<Mat44<f32>>
    + ShaderDataBackend<Vec4<f32>>
    + ShaderDataBackend<u32>
    + for<'a> InstanceSliceBackend<'a, Vertex, u16, Instance, Interleaved, Instance>
    + PipelineShaderData<Mat44<f32>>
    + PipelineShaderData<Vec4<f32>>
    + PipelineShaderData<u32>
    + for<'a> Uniformable<'a, f32, Target = f32>
    + for<'a> Uniformable<'a, Vec2<f32>, Target = Vec2<f32>>
    + for<'a> Uniformable<'a, Vec3<f32>, Target = Vec3<f32>>
    + for<'a> Uniformable<'a, Vec4<f32>, Target = Vec4<f32>>
    + for<'a> Uniformable<'a, Mat44<f32>, Target = Mat44<f32>>
    + for<'a> Uniformable<'a, ShaderDataBinding<Vec4<f32>>, Target = ShaderDataBinding<Vec4<f32>>>
    + for<'a> Uniformable<'a, ShaderDataBinding<Mat44<f32>>, Target = ShaderDataBinding<Mat44<f32>>>
    + for<'a> Uniformable<'a, ShaderDataBinding<u32>, Target = ShaderDataBinding<u32>>
{
}

impl<B: ?Sized> WireframeBackend for B where
    B: TessBackend<Vertex, u16, (), Interleaved>
        + TessBackend<Vertex, u16, Instance, Interleaved>
        + TessBackend<(), u16, (), Interleaved>
        + ShaderBackend
        + FramebufferBackend<Dim2>
        + PipelineBackend<Dim2>
        + RenderGateBackend
        + TessGateBackend<Vertex, u16, (), Interleaved>
        + TessGateBackend<Vertex, u16, Instance, Interleaved>
        + TessGateBackend<(), u16, (), Interleaved>
        + ShaderDataBackend<Mat44<f32>>
        + ShaderDataBackend<Vec4<f32>>
        + ShaderDataBackend<u32>
        + for<'a> InstanceSliceBackend<'a, Vertex, u16, Instance, Interleaved, Instance>
        + PipelineShaderData<Mat44<f32>>
        + PipelineShaderData<Vec4<f32>>
        + PipelineShaderData<u32>
        + for<'a> Uniformable<'a, f32, Target = f32>
        + for<'a> Uniformable<'a, Vec2<f32>, Target = Vec2<f32>>
        + for<'a> Uniformable<'a, Vec3<f32>, Target = Vec3<f32>>
        + for<'a> Uniformable<'a, Vec4<f32>, Target = Vec4<f32>>
        + for<'a> Uniformable<'a, Mat44<f32>, Target = Mat44<f32>>
        + for<'a> Uniformable<'a, ShaderDataBinding<Vec4<f32>>, Target = ShaderDataBinding<Vec4<f32>>>
        + for<'a> Uniformable<
            'a,
            ShaderDataBinding<Mat44<f32>>,
            Target = ShaderDataBinding<Mat44<f32>>,
        > + for<'a> Uniformable<'a, ShaderDataBinding<u32>, Target = ShaderDataBinding<u32>>
{
}

pub struct WireframeRenderer<B>
where
    B: WireframeBackend,
{
    static_shader: Program<B, VertexSemantics, (), Uniforms>,
    dynamic_shader: Program<B, VertexSemantics, (), Uniforms>,
    line_shader: Program<B, VertexSemantics, (), Uniforms>,

    static_tess: Arena<StaticWireframeTess<B>>,
    dynamic_tess: Arena<DynamicWireframeTess<B>>,

    static_list: Vec<(StaticWireframeTessId, WireframeInstance)>,

    line_positions: Vec<Vec4<f32>>,
    line_colors: Vec<Vec4<f32>>,
    line_commands: Vec<LineCommand>,
    // Indicates cutoffs of < LINE_BUFFER_SIZE vertices.
    line_batches: Vec<usize>,
    // Current position in the chunk of LINE_BUFFER_SIZE vertices.
    line_pos: usize,
    line_batch_counter: usize,

    tx_buffer: ShaderData<B, Mat44<f32>>,

    line_positions_halfway: Vec<Vec4<f32>>,
    line_colors_halfway: Vec<Vec4<f32>>,
    line_indices_halfway: Vec<u32>,

    line_position_buffer: ShaderData<B, Vec4<f32>>,
    line_color_buffer: ShaderData<B, Vec4<f32>>,
    line_index_buffer: ShaderData<B, u32>,
    line_tess: Tess<B, (), u16, (), Interleaved>,

    pub projection: Matrix4<f32>,
    pub view: Matrix4<f32>,
    pub target_size: Vector2<f32>,

    fog_distance: f32,
    light_direction: Vector3<f32>,
    light_diffuse_color: LinearColor,
    light_back_color: LinearColor,
    light_ambient_color: LinearColor,

    line_thickness: f32,
}

// Max number of instances of a dynamic tess we ever expect to render at once.
//
// Needs to match the size of `u_Txs` in our dynamic tess vertex shader.
const TX_BUFFER_SIZE: usize = 1024;
const LINE_BUFFER_SIZE: usize = 1024;

impl<B> WireframeRenderer<B>
where
    B: WireframeBackend,
{
    pub fn new(
        ctx: &mut impl GraphicsContext<Backend = B>,
        target_size: Vector2<f32>,
    ) -> Result<Self> {
        let static_shader = ctx
            .new_shader_program()
            .from_strings(STATIC_VERTEX_SRC, None, None, FRAGMENT_SRC)
            .with_context(|| "while building static wireframe vertex shader")?
            .ignore_warnings();
        let dynamic_shader = ctx
            .new_shader_program()
            .from_strings(DYNAMIC_VERTEX_SRC, None, None, FRAGMENT_SRC)
            .with_context(|| "while building dynamic wireframe vertex shader")?
            .ignore_warnings();
        let line_shader = ctx
            .new_shader_program()
            .from_strings(LINE_VERTEX_SRC, None, None, LINE_FRAGMENT_SRC)
            .with_context(|| "while building line wireframe vertex shader")?
            .ignore_warnings();

        let tx_buffer = ctx.new_shader_data(vec![Mat44([[0.; 4]; 4]); TX_BUFFER_SIZE])?;

        let line_position_buffer = ctx.new_shader_data(vec![Vec4([0.; 4]); LINE_BUFFER_SIZE])?;
        let line_color_buffer = ctx.new_shader_data(vec![Vec4([0.; 4]); LINE_BUFFER_SIZE])?;
        let line_index_buffer = ctx.new_shader_data(vec![0u32; LINE_BUFFER_SIZE])?;

        // Note: no data; this `Tess` is for attributeless rendering of lines.
        let line_tess = TessBuilder::build(
            TessBuilder::new(ctx)
                // In any one batch we may have up to `6 * LINE_BUFFER_SIZE` vertices.
                .set_render_vertex_nb(6 * LINE_BUFFER_SIZE)
                .set_mode(Mode::Triangle),
        )?;

        Ok(Self {
            static_shader,
            dynamic_shader,
            line_shader,
            static_tess: Arena::new(),
            dynamic_tess: Arena::new(),
            static_list: Vec::new(),
            line_positions: Vec::new(),
            line_colors: Vec::new(),
            line_commands: Vec::new(),
            line_batches: Vec::new(),
            line_pos: 0,
            line_batch_counter: 0,
            tx_buffer,
            line_positions_halfway: vec![Vec4([0.; 4]); LINE_BUFFER_SIZE],
            line_colors_halfway: vec![Vec4([0.; 4]); LINE_BUFFER_SIZE],
            line_indices_halfway: vec![0; LINE_BUFFER_SIZE],
            line_position_buffer,
            line_color_buffer,
            line_index_buffer,
            line_tess,
            projection: Matrix4::identity(),
            view: Matrix4::identity(),
            fog_distance: 64.,
            light_direction: -(Vector3::x() + Vector3::y() + Vector3::z()).normalize(),
            light_diffuse_color: LinearColor {
                r: 0.8,
                g: 0.8,
                b: 0.8,
                a: 1.0,
            },
            light_back_color: LinearColor {
                r: 0.1,
                g: 0.1,
                b: 0.1,
                a: 1.0,
            },
            light_ambient_color: LinearColor {
                r: 0.1,
                g: 0.1,
                b: 0.1,
                a: 1.0,
            },
            line_thickness: 1.0,
            target_size,
        })
    }
}

impl<B> WireframeRenderer<B>
where
    B: WireframeBackend,
{
    pub fn insert_static_tess(
        &mut self,
        tess: Tess<B, Vertex, u16, (), Interleaved>,
        enable_lighting: bool,
    ) -> StaticWireframeTessId {
        StaticWireframeTessId(self.static_tess.insert(StaticWireframeTess {
            tess,
            enable_lighting,
        }))
    }

    /// `initial_size` is the maximum number of instances which you expect to have to render. It is
    /// possible that the number could be too large. If that's the case,
    pub fn insert_dynamic_tess(
        &mut self,
        tess: Tess<B, Vertex, u16, Instance, Interleaved>,
        enable_lighting: bool,
    ) -> DynamicWireframeTessId {
        let dynamic_tess = DynamicWireframeTess {
            tess,
            enable_lighting,
            instance_list: Vec::new(),
        };
        DynamicWireframeTessId(self.dynamic_tess.insert(dynamic_tess))
    }

    pub fn get_static_tess(&self, id: StaticWireframeTessId) -> Option<&StaticWireframeTess<B>> {
        self.static_tess.get(id.0)
    }

    pub fn get_dynamic_tess(&self, id: DynamicWireframeTessId) -> Option<&DynamicWireframeTess<B>> {
        self.dynamic_tess.get(id.0)
    }

    pub fn get_static_tess_mut(
        &mut self,
        id: StaticWireframeTessId,
    ) -> Option<&mut StaticWireframeTess<B>> {
        self.static_tess.get_mut(id.0)
    }

    pub fn get_dynamic_tess_mut(
        &mut self,
        id: DynamicWireframeTessId,
    ) -> Option<&mut DynamicWireframeTess<B>> {
        self.dynamic_tess.get_mut(id.0)
    }

    pub fn remove_static_tess(
        &mut self,
        id: StaticWireframeTessId,
    ) -> Option<Tess<B, Vertex, u16, (), Interleaved>> {
        self.static_tess.remove(id.0).map(|wt| wt.tess)
    }

    pub fn remove_dynamic_tess(
        &mut self,
        id: DynamicWireframeTessId,
    ) -> Option<Tess<B, Vertex, u16, Instance, Interleaved>> {
        self.dynamic_tess.remove(id.0).map(|wt| wt.tess)
    }

    pub fn clear_draw_queue(&mut self) {
        self.static_list.clear();
        for (_, dw) in self.dynamic_tess.iter_mut() {
            dw.instance_list.clear();
        }
        self.line_positions.clear();
        self.line_colors.clear();
        self.line_commands.clear();
        self.line_batches.clear();
        self.line_pos = 0;
        self.line_batch_counter = 0;
    }

    pub fn queue_draw_static(
        &mut self,
        static_tess_id: StaticWireframeTessId,
        instance: WireframeInstance,
    ) {
        self.static_list.push((static_tess_id, instance));
    }

    pub fn queue_draw_dynamic(
        &mut self,
        dynamic_tess_id: DynamicWireframeTessId,
        instance: WireframeInstance,
    ) {
        self.dynamic_tess[dynamic_tess_id.0].queue_instance(instance);
    }

    pub fn queue_draw_line(&mut self, a: LineVertex, b: LineVertex) {
        // positions and colors are in lockstep
        let start = self.line_positions.len();
        self.line_positions
            .extend([Vec4(a.pos.push(1.).into()), Vec4(b.pos.push(1.).into())]);
        self.line_colors
            .extend([Vec4(a.color.into()), Vec4(b.color.into())]);

        if self.line_pos + 4 >= LINE_BUFFER_SIZE {
            self.line_batches.push(self.line_batch_counter);
            self.line_batch_counter = 0;
            self.line_pos = 0;
        }

        self.line_batch_counter += 1;
        self.line_pos += 4;
        self.line_commands.push(LineCommand {
            start,
            len: 2,
            cap: true,
        });
    }

    pub fn queue_draw_line_strip(&mut self, vertices: impl IntoIterator<Item = LineVertex>) {
        // positions and colors are in lockstep
        let start = self.line_positions.len();
        for v in vertices {
            self.line_positions.push(Vec4(v.pos.push(1.).into()));
            self.line_colors.push(Vec4(v.color.into()));
        }
        let end = self.line_positions.len();
        let len = end - start;

        if self.line_pos + len + 2 >= LINE_BUFFER_SIZE {
            self.line_batches.push(self.line_batch_counter);
            self.line_batch_counter = 0;
            self.line_pos = 0;
        }

        self.line_batch_counter += 1;
        self.line_pos += len + 2;
        self.line_commands.push(LineCommand {
            start,
            len,
            cap: true,
        });
    }

    pub fn queue_draw_line_loop<I>(&mut self, vertices: I)
    where
        I: IntoIterator<Item = LineVertex>,
        I::IntoIter: DoubleEndedIterator,
    {
        let mut vertices = vertices.into_iter();
        let first = vertices
            .next()
            .expect("line loop must have at least two vertices!");
        let last = vertices
            .next_back()
            .expect("line loop must have at least two vertices!");

        // positions and colors are in lockstep
        let start = self.line_positions.len();
        for v in [last, first]
            .into_iter()
            .chain(vertices)
            .chain([last, first])
        {
            self.line_positions.push(Vec4(v.pos.push(1.).into()));
            self.line_colors.push(Vec4(v.color.into()));
        }
        let end = self.line_positions.len();
        let len = end - start;

        if self.line_pos + len >= LINE_BUFFER_SIZE {
            self.line_batches.push(self.line_batch_counter);
            self.line_batch_counter = 0;
            self.line_pos = 0;
        }

        self.line_batch_counter += 1;
        self.line_pos += len;
        self.line_commands.push(LineCommand {
            start,
            len,
            cap: false,
        });
    }

    // pub fn queue_draw_line_strip(&mut self, vertices: impl IntoIterator<Item = LineVertex>) {
    //     let start = self.line_vertex_buffer.len().try_into().unwrap();
    //     let mut vertex_iter = vertices.into_iter();
    //     let mut current = vertex_iter
    //         .next()
    //         .expect("a line strip must have at least two vertices!")
    //         .to_array();
    //     self.line_vertices.push(current);
    //     current = vertex_iter
    //         .next()
    //         .expect("a line strip must have at least two vertices!")
    //         .to_array();
    //     for next in vertex_iter {
    //         self.line_vertices.push(current);
    //         current = next.to_array();
    //     }

    //     let end = self.line_strip_vertices.len().try_into().unwrap();
    //     self.push_line_strip(start..end);
    // }

    pub fn set_fog_distance(&mut self, fog_distance: f32) {
        self.fog_distance = fog_distance;
    }

    pub fn set_light_direction(&mut self, dir: Vector3<f32>) {
        self.light_direction = dir;
    }

    pub fn set_light_diffuse_color(&mut self, diffuse: LinearColor) {
        self.light_diffuse_color = diffuse;
    }

    pub fn set_light_backlight_color(&mut self, back: LinearColor) {
        self.light_back_color = back;
    }

    pub fn set_light_ambient_color(&mut self, ambient: LinearColor) {
        self.light_ambient_color = ambient;
    }

    pub fn set_target_size(&mut self, target_size: Vector2<f32>) {
        self.target_size = target_size;
    }

    pub fn set_line_thickness(&mut self, thickness: f32) {
        self.line_thickness = thickness;
    }
}

impl<B> ops::Index<StaticWireframeTessId> for WireframeRenderer<B>
where
    B: WireframeBackend,
{
    type Output = StaticWireframeTess<B>;

    fn index(&self, index: StaticWireframeTessId) -> &Self::Output {
        &self.static_tess[index.0]
    }
}

impl<B> ops::IndexMut<StaticWireframeTessId> for WireframeRenderer<B>
where
    B: WireframeBackend,
{
    fn index_mut(&mut self, index: StaticWireframeTessId) -> &mut Self::Output {
        &mut self.static_tess[index.0]
    }
}

impl<B> ops::Index<DynamicWireframeTessId> for WireframeRenderer<B>
where
    B: WireframeBackend,
{
    type Output = DynamicWireframeTess<B>;

    fn index(&self, index: DynamicWireframeTessId) -> &Self::Output {
        &self.dynamic_tess[index.0]
    }
}

impl<B> ops::IndexMut<DynamicWireframeTessId> for WireframeRenderer<B>
where
    B: WireframeBackend,
{
    fn index_mut(&mut self, index: DynamicWireframeTessId) -> &mut Self::Output {
        &mut self.dynamic_tess[index.0]
    }
}

impl<B> WireframeRenderer<B>
where
    B: WireframeBackend,
{
    pub fn draw_queued(
        &mut self,
        pipeline: &mut Pipeline<B>,
        shading_gate: &mut ShadingGate<B>,
    ) -> Result<(), PipelineError> {
        // Default should actually be just right here; it has the proper depth test
        // config and such, no scissoring, blending off.
        let render_state = RenderState::default().set_face_culling(FaceCulling::new(
            FaceCullingOrder::CCW,
            FaceCullingMode::Back,
        ));

        let view_projection = self.projection * self.view;

        shading_gate.shade(
            &mut self.static_shader,
            |mut program_interface, uni, mut render_gate| {
                program_interface.set(&uni.view, Mat44(self.view.into()));
                program_interface.set(&uni.fog_distance, self.fog_distance);
                program_interface.set(&uni.light_direction, Vec3(self.light_direction.into()));

                render_gate.render(&render_state, |mut tess_gate| {
                    let queued_static_instances = self.static_list.iter();
                    for (static_tess, instance) in queued_static_instances
                        .filter_map(|(id, inst)| self.static_tess.get(id.0).map(|t| (t, inst)))
                    {
                        program_interface.set(&uni.tx, Mat44(instance.tx.into()));
                        program_interface
                            .set(&uni.mvp, Mat44((view_projection * instance.tx).into()));
                        program_interface.set(&uni.color, Vec4(instance.color.into()));

                        if static_tess.enable_lighting {
                            program_interface.set(
                                &uni.light_diffuse_color,
                                Vec3(self.light_diffuse_color.into()),
                            );
                            program_interface
                                .set(&uni.light_back_color, Vec3(self.light_back_color.into()));
                            program_interface.set(
                                &uni.light_ambient_color,
                                Vec3(self.light_ambient_color.into()),
                            );
                        } else {
                            program_interface
                                .set(&uni.light_diffuse_color, Vec3(LinearColor::BLACK.into()));
                            program_interface
                                .set(&uni.light_back_color, Vec3(LinearColor::BLACK.into()));
                            program_interface
                                .set(&uni.light_ambient_color, Vec3(LinearColor::WHITE.into()));
                        }

                        let tess_view = TessView::whole(&static_tess.tess);
                        tess_gate.render(tess_view)?;
                    }

                    Ok(())
                })
            },
        )?;

        shading_gate.shade(
            &mut self.dynamic_shader,
            |mut program_interface, uni, mut render_gate| {
                program_interface.set(&uni.view, Mat44(self.view.into()));
                program_interface.set(&uni.mvp, Mat44(view_projection.into()));
                program_interface.set(&uni.fog_distance, self.fog_distance);
                program_interface.set(&uni.light_direction, Vec3(self.light_direction.into()));

                render_gate.render(&render_state, |mut tess_gate| {
                    for (_, dynamic_tess) in self.dynamic_tess.iter_mut() {
                        if dynamic_tess.instance_list.is_empty() {
                            // For some reason, Luminance still wants to render if there are no
                            // instances.
                            continue;
                        }

                        if dynamic_tess.enable_lighting {
                            program_interface.set(
                                &uni.light_diffuse_color,
                                Vec3(self.light_diffuse_color.into()),
                            );
                            program_interface
                                .set(&uni.light_back_color, Vec3(self.light_back_color.into()));
                            program_interface.set(
                                &uni.light_ambient_color,
                                Vec3(self.light_ambient_color.into()),
                            );
                        } else {
                            program_interface
                                .set(&uni.light_diffuse_color, Vec3(LinearColor::BLACK.into()));
                            program_interface
                                .set(&uni.light_back_color, Vec3(LinearColor::BLACK.into()));
                            program_interface
                                .set(&uni.light_ambient_color, Vec3(LinearColor::WHITE.into()));
                        }

                        for instance_batch in dynamic_tess.instance_list.chunks(TX_BUFFER_SIZE) {
                            let mut instances = dynamic_tess.tess.instances_mut().unwrap();
                            for (i, instance) in instance_batch.iter().enumerate() {
                                instances[i].color = InstanceColor::new(instance.color.into());
                                self.tx_buffer.set(i, Mat44(instance.tx.into())).unwrap();
                            }
                            drop(instances);

                            let txs_binding = pipeline.bind_shader_data(&mut self.tx_buffer)?;
                            program_interface.set(&uni.instance_txs, txs_binding.binding());

                            let tess_view =
                                TessView::inst_whole(&dynamic_tess.tess, instance_batch.len());

                            tess_gate.render(tess_view)?;
                        }
                    }

                    Ok(())
                })?;

                Ok(())
            },
        )?;

        shading_gate.shade(
            &mut self.line_shader,
            |mut program_interface, uni, mut render_gate| {
                program_interface.set(&uni.view, Mat44(self.view.into()));
                program_interface.set(&uni.mvp, Mat44(view_projection.into()));
                program_interface.set(&uni.fog_distance, self.fog_distance);
                program_interface.set(&uni.thickness, self.line_thickness);
                program_interface.set(&uni.resolution, Vec2(self.target_size.into()));

                render_gate.render(&render_state.set_face_culling(None), |mut tess_gate| {
                    let mut commands = self.line_commands.iter().copied();
                    // The final `usize::MAX` on the end here is to drain any last commands that
                    // weren't chunked. There should be less than a full chunk (since if there were
                    // more they would already have caused a chunk to happen.)
                    let batches = self.line_batches.iter().copied().chain(Some(usize::MAX));
                    for batch in batches {
                        // For each command, we end up with a string of vertices which we
                        // insert, and a string of indices. For a given command of length N
                        // (where N must be > 1), we'll end up with N-1 line segments, and thus
                        // N-1 line indices. We'll *also* end up with N+2 vertices.
                        //
                        // The line index corresponds physically to the index of the
                        // *predecessor* vertex of the line segment it represents; each line
                        // index is essentially a window of four vertices.
                        //
                        // We begin a batch w/ the vertex index at the start of the vertex buffer
                        // and with the line index in the same position.
                        let mut vertex_index = 0;
                        let mut line_index = 0;
                        for command in commands.by_ref().take(batch) {
                            let n_segments;
                            let n_vertices;

                            if command.cap {
                                n_segments = command.len - 1;
                                n_vertices = command.len + 2;

                                // Begin with the first "cap". Every non-loop strip of line segments
                                // will be "capped". Then, copy in all of the "inner" vertices, and
                                // finally add the end cap. The start and end caps are copies of the
                                // elements immediately after and before them, respectively.
                                self.line_positions_halfway[vertex_index] =
                                    self.line_positions[command.start];
                                self.line_positions_halfway
                                    [vertex_index + 1..vertex_index + 1 + command.len]
                                    .copy_from_slice(
                                        &self.line_positions[command.start..][..command.len],
                                    );
                                self.line_positions_halfway[vertex_index + n_vertices - 1] =
                                    self.line_positions[command.start + command.len - 1];

                                self.line_colors_halfway[vertex_index] =
                                    self.line_colors[command.start];
                                self.line_colors_halfway
                                    [vertex_index + 1..vertex_index + 1 + command.len]
                                    .copy_from_slice(
                                        &self.line_colors[command.start..][..command.len],
                                    );
                                self.line_colors_halfway[vertex_index + n_vertices - 1] =
                                    self.line_colors[command.start + command.len - 1];
                            } else {
                                n_segments = command.len - 2;
                                n_vertices = command.len;

                                self.line_positions_halfway
                                    [vertex_index..vertex_index + command.len]
                                    .copy_from_slice(
                                        &self.line_positions[command.start..][..command.len],
                                    );

                                self.line_colors_halfway[vertex_index..vertex_index + command.len]
                                    .copy_from_slice(
                                        &self.line_colors[command.start..][..command.len],
                                    );
                            }

                            // Now that we have the vertices in, we need to deal with the line
                            // indices. These are simpler; they're just the range
                            // `vertex_index..vertex_index + n_segments`, and we can just plop them
                            // right into the index buffer at `line_index..line_index + n_segments`.
                            self.line_indices_halfway[line_index..line_index + n_segments]
                                .iter_mut()
                                .zip(vertex_index..vertex_index + n_segments)
                                .for_each(|(dst, idx)| *dst = idx as u32);

                            // We're done copying data - update the current vertices/segment
                            // indices.
                            vertex_index += n_vertices;
                            line_index += n_segments;
                        }

                        self.line_position_buffer
                            .replace(self.line_positions_halfway.iter().copied())
                            .unwrap();
                        self.line_color_buffer
                            .replace(self.line_colors_halfway.iter().copied())
                            .unwrap();
                        self.line_index_buffer
                            .replace(self.line_indices_halfway.iter().copied())
                            .unwrap();

                        let bound_positions =
                            pipeline.bind_shader_data(&mut self.line_position_buffer)?;
                        let bound_colors =
                            pipeline.bind_shader_data(&mut self.line_color_buffer)?;
                        let bound_indices =
                            pipeline.bind_shader_data(&mut self.line_index_buffer)?;

                        program_interface.set(&uni.positions, bound_positions.binding());
                        program_interface.set(&uni.colors, bound_colors.binding());
                        program_interface.set(&uni.indices, bound_indices.binding());

                        // Attributeless render, go!!
                        let view = TessView::sub(&self.line_tess, 6 * line_index).unwrap();
                        tess_gate.render(view)?;
                    }

                    Ok(())
                })?;

                Ok(())
            },
        )?;

        Ok(())
    }
}

pub fn collect_wireframes<B>(
    context: SystemContext,
    wireframe_renderer: &mut WireframeRenderer<B>,
    dynamic_wireframes: &mut PreparedQuery<(
        Or<&DynamicWireframe, &StaticWireframe>,
        &Transform,
        Option<&Color>,
    )>,
) where
    B: WireframeBackend,
{
    for (_, (wireframe, transform, maybe_color)) in
        context.prepared_query(dynamic_wireframes).iter()
    {
        let instance = WireframeInstance {
            color: maybe_color
                .copied()
                .map(LinearColor::from)
                .unwrap_or(LinearColor::WHITE),
            tx: transform.tx,
        };

        if let Some(dynamic_wireframe) = wireframe.left() {
            wireframe_renderer.queue_draw_dynamic(dynamic_wireframe.tess_id, instance);
        }

        if let Some(static_wireframe) = wireframe.right() {
            wireframe_renderer.queue_draw_static(static_wireframe.tess_id, instance);
        }
    }
}
