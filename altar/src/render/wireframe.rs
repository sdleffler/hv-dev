use crate::render::{pipeline::semantics::*, Color, LinearColor, Transform};
use hv::{
    ecs::{Or, PreparedQuery, SystemContext},
    prelude::*,
};
use luminance::{
    backend::{
        buffer::{Buffer as BufferBackend, BufferSlice as BufferSliceBackend},
        framebuffer::Framebuffer as FramebufferBackend,
        pipeline::{Pipeline as PipelineBackend, PipelineBuffer},
        render_gate::RenderGate as RenderGateBackend,
        shader::{Shader as ShaderBackend, Uniformable},
        tess::{InstanceSlice as InstanceSliceBackend, Tess as TessBackend},
        tess_gate::TessGate as TessGateBackend,
    },
    buffer::Buffer,
    context::GraphicsContext,
    face_culling::{FaceCulling, FaceCullingMode, FaceCullingOrder},
    pipeline::{BufferBinding, Pipeline, PipelineError},
    render_state::RenderState,
    shader::{Program, Uniform},
    shading_gate::ShadingGate,
    tess::{Interleaved, Tess, TessView},
    texture::Dim2,
    UniformInterface, Vertex,
};
use thunderdome::{Arena, Index};

#[derive(Clone, Copy, Debug, Vertex)]
#[vertex(sem = "VertexSemantics")]
pub struct Vertex {
    pub position: VertexPosition,
    #[vertex(normalized = true)]
    pub color: VertexColor,
    pub normal: VertexNormal,
}

#[derive(Clone, Copy, Debug, Vertex)]
#[vertex(sem = "VertexSemantics", instanced = true)]
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
    pub tx: Uniform<[[f32; 4]; 4]>,
    #[uniform(unbound, name = "u_View")]
    pub view: Uniform<[[f32; 4]; 4]>,
    #[uniform(unbound, name = "u_MVP")]
    pub mvp: Uniform<[[f32; 4]; 4]>,
    #[uniform(unbound, name = "u_Color")]
    pub color: Uniform<[f32; 4]>,
    #[uniform(unbound, name = "u_FogDistance")]
    pub fog_distance: Uniform<f32>,
    #[uniform(unbound, name = "u_LightDirection")]
    pub light_direction: Uniform<[f32; 3]>,
    #[uniform(unbound, name = "u_LightDiffuseColor")]
    pub light_diffuse_color: Uniform<[f32; 3]>,
    #[uniform(unbound, name = "u_LightBackColor")]
    pub light_back_color: Uniform<[f32; 3]>,
    #[uniform(unbound, name = "u_LightAmbientColor")]
    pub light_ambient_color: Uniform<[f32; 3]>,
    #[uniform(unbound, name = "u_InstanceTxs")]
    pub instance_txs: Uniform<BufferBinding<Matrix4<f32>>>,
}

pub const STATIC_VERTEX_SRC: &str = include_str!("wireframe/static_wireframe_es300.glslv");
pub const DYNAMIC_VERTEX_SRC: &str = include_str!("wireframe/dynamic_wireframe_es300.glslv");
pub const FRAGMENT_SRC: &str = include_str!("wireframe/wireframe_es300.glslf");

pub struct StaticWireframeTess<B: ?Sized>
where
    B: TessBackend<Vertex, u16, (), Interleaved>,
{
    tess: Tess<B, Vertex, u16, (), Interleaved>,
    enable_lighting: bool,
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
    B: TessBackend<Vertex, u16, Instance, Interleaved>,
    B: BufferBackend<Matrix4<f32>>,
{
    tess: Tess<B, Vertex, u16, Instance, Interleaved>,
    enable_lighting: bool,
    instance_list: Vec<WireframeInstance>,
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

pub trait WireframeBackend:
    TessBackend<Vertex, u16, (), Interleaved>
    + TessBackend<Vertex, u16, Instance, Interleaved>
    + ShaderBackend
    + PipelineBackend<Dim2>
    + FramebufferBackend<Dim2>
    + RenderGateBackend
    + TessGateBackend<Vertex, u16, (), Interleaved>
    + TessGateBackend<Vertex, u16, Instance, Interleaved>
    + BufferBackend<Matrix4<f32>>
    + InstanceSliceBackend<Vertex, u16, Instance, Interleaved, Instance>
    + BufferSliceBackend<Matrix4<f32>>
    + PipelineBuffer<Matrix4<f32>>
{
}

impl<B: ?Sized> WireframeBackend for B where
    B: TessBackend<Vertex, u16, (), Interleaved>
        + TessBackend<Vertex, u16, Instance, Interleaved>
        + ShaderBackend
        + FramebufferBackend<Dim2>
        + PipelineBackend<Dim2>
        + RenderGateBackend
        + TessGateBackend<Vertex, u16, (), Interleaved>
        + TessGateBackend<Vertex, u16, Instance, Interleaved>
        + BufferBackend<Matrix4<f32>>
        + InstanceSliceBackend<Vertex, u16, Instance, Interleaved, Instance>
        + BufferSliceBackend<Matrix4<f32>>
        + PipelineBuffer<Matrix4<f32>>
{
}

pub struct WireframeRenderer<B: ?Sized>
where
    B: WireframeBackend,
{
    static_shader: Program<B, VertexSemantics, (), Uniforms>,
    dynamic_shader: Program<B, VertexSemantics, (), Uniforms>,
    static_tess: Arena<StaticWireframeTess<B>>,
    dynamic_tess: Arena<DynamicWireframeTess<B>>,
    static_list: Vec<(StaticWireframeTessId, WireframeInstance)>,
    tx_buffer: Buffer<B, Matrix4<f32>>,
    projection: Matrix4<f32>,
    view: Matrix4<f32>,
    fog_distance: f32,
    light_direction: Vector3<f32>,
    light_diffuse_color: LinearColor,
    light_back_color: LinearColor,
    light_ambient_color: LinearColor,
}

// Max number of instances of a dynamic tess we ever expect to render at once.
//
// Needs to match the size of `u_Txs` in our dynamic tess vertex shader.
const TX_BUFFER_SIZE: usize = 1024;

impl<B: ?Sized> WireframeRenderer<B>
where
    B: WireframeBackend,
    f32: Uniformable<B>,
    [f32; 3]: Uniformable<B>,
    [f32; 4]: Uniformable<B>,
    [[f32; 4]; 4]: Uniformable<B>,
    BufferBinding<Matrix4<f32>>: Uniformable<B>,
{
    pub fn new(ctx: &mut impl GraphicsContext<Backend = B>) -> Result<Self> {
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

        let tx_buffer = ctx.new_buffer(TX_BUFFER_SIZE)?;

        Ok(Self {
            static_shader,
            dynamic_shader,
            static_tess: Arena::new(),
            dynamic_tess: Arena::new(),
            static_list: Vec::new(),
            tx_buffer,
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

    pub fn clear_draw_queue(&mut self) {
        self.static_list.clear();
        for (_, dw) in self.dynamic_tess.iter_mut() {
            dw.instance_list.clear();
        }
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
        self.dynamic_tess[dynamic_tess_id.0]
            .instance_list
            .push(instance);
    }

    pub fn set_projection(&mut self, matrix: &Matrix4<f32>) {
        self.projection = *matrix;
    }

    pub fn set_view(&mut self, matrix: &Matrix4<f32>) {
        self.view = *matrix;
    }

    pub fn set_fog_distance(&mut self, fog_distance: f32) {
        self.fog_distance = fog_distance;
    }

    pub fn set_light_direction(&mut self, dir: Vector3<f32>) {
        self.light_direction = dir;
    }

    pub fn set_light_diffuse_color(&mut self, diffuse: LinearColor) {
        self.light_diffuse_color = diffuse;
    }

    pub fn set_light_ambient_color(&mut self, ambient: LinearColor) {
        self.light_ambient_color = ambient;
    }
}

impl<B> WireframeRenderer<B>
where
    B: WireframeBackend,
    f32: Uniformable<B>,
    [f32; 3]: Uniformable<B>,
    [f32; 4]: Uniformable<B>,
    [[f32; 4]; 4]: Uniformable<B>,
    BufferBinding<Matrix4<f32>>: Uniformable<B>,
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
        // let render_state = RenderState::default();

        let view_projection = self.projection * self.view;

        shading_gate.shade(
            &mut self.static_shader,
            |mut program_interface, uni, mut render_gate| {
                program_interface.set(&uni.view, self.view.into());
                program_interface.set(&uni.fog_distance, self.fog_distance);
                program_interface.set(&uni.light_direction, self.light_direction.into());

                render_gate.render(&render_state, |mut tess_gate| {
                    for &(static_tess_id, instance) in &self.static_list {
                        program_interface.set(&uni.tx, instance.tx.into());
                        program_interface.set(&uni.mvp, (view_projection * instance.tx).into());
                        program_interface.set(&uni.color, instance.color.into());

                        let static_tess = &self.static_tess[static_tess_id.0];
                        if static_tess.enable_lighting {
                            program_interface
                                .set(&uni.light_diffuse_color, self.light_diffuse_color.into());
                            program_interface
                                .set(&uni.light_back_color, self.light_back_color.into());
                            program_interface
                                .set(&uni.light_ambient_color, self.light_ambient_color.into());
                        } else {
                            program_interface
                                .set(&uni.light_diffuse_color, LinearColor::BLACK.into());
                            program_interface.set(&uni.light_back_color, LinearColor::BLACK.into());
                            program_interface
                                .set(&uni.light_ambient_color, LinearColor::WHITE.into());
                        }

                        let tess = &self.static_tess[static_tess_id.0].tess;
                        let tess_view = TessView::whole(tess);
                        tess_gate.render(tess_view)?;
                    }

                    Ok(())
                })
            },
        )?;

        shading_gate.shade(
            &mut self.dynamic_shader,
            |mut program_interface, uni, mut render_gate| {
                program_interface.set(&uni.view, self.view.into());
                program_interface.set(&uni.mvp, view_projection.into());
                program_interface.set(&uni.fog_distance, self.fog_distance);
                program_interface.set(&uni.light_direction, self.light_direction.into());

                render_gate.render(&render_state, |mut tess_gate| {
                    for (_, dynamic_tess) in self.dynamic_tess.iter_mut() {
                        let mut instances = dynamic_tess.tess.instances_mut().unwrap();
                        let mut txs = self.tx_buffer.slice_mut().unwrap();
                        for (i, instance) in dynamic_tess.instance_list.iter().enumerate() {
                            instances[i].color = InstanceColor::new(instance.color.into());
                            txs[i] = instance.tx;
                        }
                        drop(txs);
                        drop(instances);

                        if dynamic_tess.enable_lighting {
                            program_interface
                                .set(&uni.light_diffuse_color, self.light_diffuse_color.into());
                            program_interface
                                .set(&uni.light_back_color, self.light_back_color.into());
                            program_interface
                                .set(&uni.light_ambient_color, self.light_ambient_color.into());
                        } else {
                            program_interface
                                .set(&uni.light_diffuse_color, LinearColor::BLACK.into());
                            program_interface.set(&uni.light_back_color, LinearColor::BLACK.into());
                            program_interface
                                .set(&uni.light_ambient_color, LinearColor::WHITE.into());
                        }

                        let txs_binding = pipeline.bind_buffer(&mut self.tx_buffer)?;
                        program_interface.set(&uni.instance_txs, txs_binding.binding());

                        let tess_view = TessView::inst_whole(
                            &dynamic_tess.tess,
                            dynamic_tess.instance_list.len(),
                        );

                        tess_gate.render(tess_view)?;
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
