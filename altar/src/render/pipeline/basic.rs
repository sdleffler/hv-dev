use crate::render::pipeline::semantics::*;
use hv::prelude::*;
use luminance::{
    backend::shader::{Shader, Uniformable},
    context::GraphicsContext,
    pipeline::BufferBinding,
    shader::{Program, Stage, StageType, Uniform},
    UniformInterface, Vertex,
};

#[derive(Clone, Copy, Debug, Vertex)]
#[vertex(sem = "VertexSemantics")]
pub struct Vertex {
    pub position: VertexPosition,
    pub uv: VertexUv,
    #[vertex(normalized = true)]
    pub color: VertexColor,
}

#[derive(Clone, Copy, Debug, Vertex)]
#[vertex(sem = "VertexSemantics", instanced = true)]
pub struct Instance {
    pub src: InstanceSource,
    #[vertex(normalized = true)]
    pub color: InstanceColor,
}

#[derive(Debug, UniformInterface)]
pub struct Interface {
    #[uniform(unbound, name = "u_MVP")]
    pub mvp: Uniform<[[f32; 4]; 4]>,
    #[uniform(unbound, name = "a_Tx")]
    pub instance_txs: Uniform<BufferBinding<Matrix4<f32>>>,
}

pub const VERTEX_SRC: &str = include_str!("basic_es300.glslv");
pub const FRAGMENT_SRC: &str = include_str!("basic_es300.glslf");

pub fn vertex<B: ?Sized + Shader, C: GraphicsContext<Backend = B>>(
    ctx: &mut C,
) -> Result<Stage<B>> {
    ctx.new_shader_stage(StageType::VertexShader, VERTEX_SRC)
        .map_err(Into::into)
}

pub fn fragment<B: ?Sized + Shader, C: GraphicsContext<Backend = B>>(
    ctx: &mut C,
) -> Result<Stage<B>> {
    ctx.new_shader_stage(StageType::FragmentShader, FRAGMENT_SRC)
        .map_err(Into::into)
}

pub fn program<B: ?Sized + Shader, C: GraphicsContext<Backend = B>, Out>(
    ctx: &mut C,
) -> Result<Program<B, VertexSemantics, Out, Interface>>
where
    [[f32; 4]; 4]: Uniformable<B>,
    BufferBinding<Matrix4<f32>>: Uniformable<B>,
{
    let vertex_stage = vertex(ctx)?;
    let fragment_stage = fragment(ctx)?;
    Ok(ctx
        .new_shader_program::<VertexSemantics, Out, Interface>()
        .from_stages(&vertex_stage, None, None, &fragment_stage)?
        .ignore_warnings())
}
