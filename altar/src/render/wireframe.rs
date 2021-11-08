use crate::render::{pipeline::semantics::*, Color, LinearColor, Transform};
use hv::{
    ecs::{Or, QueryMarker, SystemContext},
    prelude::*,
};
use luminance::{
    backend::{
        buffer::{Buffer as BufferBackend, BufferSlice as BufferSliceBackend},
        framebuffer::Framebuffer as FramebufferBackend,
        pipeline::Pipeline as PipelineBackend,
        render_gate::RenderGate as RenderGateBackend,
        shader::{Shader as ShaderBackend, Uniformable},
        tess::{InstanceSlice as InstanceSliceBackend, Tess as TessBackend},
        tess_gate::TessGate as TessGateBackend,
    },
    buffer::Buffer,
    context::GraphicsContext,
    framebuffer::Framebuffer,
    pipeline::{BufferBinding, PipelineState},
    render_state::RenderState,
    shader::{Program, Uniform},
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
}

#[derive(Clone, Copy, Debug, Vertex)]
#[vertex(sem = "VertexSemantics", instanced = true)]
pub struct Instance {
    #[vertex(normalized = true)]
    pub color: InstanceColor,
}

#[derive(Debug, UniformInterface)]
pub struct DynamicUniforms {
    #[uniform(unbound, name = "u_MVP")]
    pub mvp: Uniform<[[f32; 4]; 4]>,
    #[uniform(unbound, name = "a_Tx")]
    pub instance_txs: Uniform<BufferBinding<Matrix4<f32>>>,
}

#[derive(Debug, UniformInterface)]
pub struct StaticUniforms {
    #[uniform(unbound, name = "u_MVP")]
    pub mvp: Uniform<[[f32; 4]; 4]>,
    #[uniform(unbound, name = "u_Tx")]
    pub tx: Uniform<[[f32; 4]; 4]>,
    #[uniform(unbound, name = "u_Color")]
    pub color: Uniform<[f32; 4]>,
}

pub const STATIC_VERTEX_SRC: &str = include_str!("wireframe/static_wireframe_es300.glslv");
pub const STATIC_FRAGMENT_SRC: &str = include_str!("wireframe/static_wireframe_es300.glslf");
pub const DYNAMIC_VERTEX_SRC: &str = include_str!("wireframe/dynamic_wireframe_es300.glslv");
pub const DYNAMIC_FRAGMENT_SRC: &str = include_str!("wireframe/dynamic_wireframe_es300.glslf");

pub struct StaticWireframeTess<B: ?Sized>
where
    B: TessBackend<Vertex, u16, (), Interleaved>,
{
    tess: Tess<B, Vertex, u16, (), Interleaved>,
}

#[derive(Debug, Clone, Copy)]
pub struct WireframeInstance {
    color: LinearColor,
    tx: Matrix4<f32>,
}

pub struct DynamicWireframeTess<B: ?Sized>
where
    B: TessBackend<Vertex, u16, Instance, Interleaved>,
    B: BufferBackend<Matrix4<f32>>,
{
    tess: Tess<B, Vertex, u16, Instance, Interleaved>,
    tx_buffer: Buffer<B, Matrix4<f32>>,
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
{
}

pub struct WireframeRenderer<B: ?Sized>
where
    B: WireframeBackend,
{
    static_shader: Program<B, VertexSemantics, (), StaticUniforms>,
    dynamic_shader: Program<B, VertexSemantics, (), DynamicUniforms>,
    static_tess: Arena<StaticWireframeTess<B>>,
    dynamic_tess: Arena<DynamicWireframeTess<B>>,
    static_list: Vec<(StaticWireframeTessId, WireframeInstance)>,
    target: Framebuffer<B, Dim2, (), ()>,
    view: Matrix4<f32>,
}

impl<B: ?Sized> WireframeRenderer<B>
where
    B: WireframeBackend,
    [f32; 4]: Uniformable<B>,
    [[f32; 4]; 4]: Uniformable<B>,
    BufferBinding<Matrix4<f32>>: Uniformable<B>,
{
    pub fn new(
        ctx: &mut impl GraphicsContext<Backend = B>,
        target: Framebuffer<B, Dim2, (), ()>,
    ) -> Result<Self> {
        let static_shader = ctx
            .new_shader_program()
            .from_strings(STATIC_VERTEX_SRC, None, None, STATIC_FRAGMENT_SRC)?
            .ignore_warnings();
        let dynamic_shader = ctx
            .new_shader_program()
            .from_strings(DYNAMIC_VERTEX_SRC, None, None, DYNAMIC_FRAGMENT_SRC)?
            .ignore_warnings();

        Ok(Self {
            static_shader,
            dynamic_shader,
            static_tess: Arena::new(),
            dynamic_tess: Arena::new(),
            static_list: Vec::new(),
            target,
            view: Matrix4::identity(),
        })
    }
}

pub fn collect_wireframes<G>(
    context: SystemContext,
    wireframe_renderer: &mut WireframeRenderer<G::Backend>,
    dynamic_wireframes: QueryMarker<(
        Or<&DynamicWireframe, &StaticWireframe>,
        &Transform,
        Option<&Color>,
    )>,
) where
    G: GraphicsContext,
    G::Backend: WireframeBackend,
{
    wireframe_renderer.static_list.clear();
    for (_, dw) in wireframe_renderer.dynamic_tess.iter_mut() {
        dw.instance_list.clear();
    }

    for (_, (wireframe, transform, maybe_color)) in context.query(dynamic_wireframes).iter() {
        let instance = WireframeInstance {
            color: maybe_color
                .copied()
                .map(LinearColor::from)
                .unwrap_or(LinearColor::WHITE),
            tx: transform.tx,
        };

        if let Some(dynamic_wireframe) = wireframe.left() {
            wireframe_renderer.dynamic_tess[dynamic_wireframe.tess_id.0]
                .instance_list
                .push(instance);
        }

        if let Some(static_wireframe) = wireframe.right() {
            wireframe_renderer
                .static_list
                .push((static_wireframe.tess_id, instance))
        }
    }
}

pub fn render_wireframes<G>(
    _context: SystemContext,
    wireframe_renderer: &mut WireframeRenderer<G::Backend>,
    graphics_context: &mut G,
    (): (),
) where
    G: GraphicsContext,
    G::Backend: WireframeBackend,
    [f32; 4]: Uniformable<G::Backend>,
    [[f32; 4]; 4]: Uniformable<G::Backend>,
    BufferBinding<Matrix4<f32>>: Uniformable<G::Backend>,
{
    // Default should actually be just right here; it has the proper depth test
    // config and such, no scissoring, blending off.
    let render_state = RenderState::default();
    let mut pipeline_gate = graphics_context.new_pipeline_gate();
    let render = pipeline_gate.pipeline(
        &wireframe_renderer.target,
        &PipelineState::default(),
        |_pipeline, mut shading_gate| {
            shading_gate.shade(
                &mut wireframe_renderer.static_shader,
                |mut program_interface, uni, mut render_gate| {
                    program_interface.set(&uni.mvp, wireframe_renderer.view.into());

                    render_gate.render(&render_state, |mut tess_gate| {
                        for &(static_tess_id, instance) in &wireframe_renderer.static_list {
                            program_interface.set(&uni.tx, instance.tx.into());
                            program_interface.set(&uni.color, instance.color.into());

                            let tess = &wireframe_renderer.static_tess[static_tess_id.0].tess;
                            let tess_view = TessView::whole(tess);
                            tess_gate.render(tess_view)?;
                        }

                        Ok(())
                    })
                },
            )?;

            shading_gate.shade(
                &mut wireframe_renderer.dynamic_shader,
                |mut program_interface, uni, mut render_gate| {
                    program_interface.set(&uni.mvp, wireframe_renderer.view.into());

                    render_gate.render(&render_state, |mut tess_gate| {
                        for (_, dynamic_tess) in wireframe_renderer.dynamic_tess.iter_mut() {
                            let mut instances = dynamic_tess.tess.instances_mut().unwrap();
                            let mut txs = dynamic_tess.tx_buffer.slice_mut().unwrap();
                            for (i, instance) in dynamic_tess.instance_list.iter().enumerate() {
                                instances[i].color = InstanceColor::new(instance.color.into());
                                txs[i] = instance.tx;
                            }

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
        },
    );
    render.assume();
}
