use std::collections::HashMap;

use hv::{
    gui::{self, epaint::Mesh16, ClippedMesh, Rect},
    prelude::*,
};
use luminance::{
    backend::{
        buffer::{Buffer as BufferBackend, BufferSlice as BufferSliceBackend},
        framebuffer::Framebuffer as FramebufferBackend,
        pipeline::{Pipeline as PipelineBackend, PipelineBuffer, PipelineTexture},
        render_gate::RenderGate as RenderGateBackend,
        shader::{Shader as ShaderBackend, Uniformable},
        tess::{IndexSlice, Tess as TessBackend, VertexSlice},
        tess_gate::TessGate as TessGateBackend,
        texture::Texture as TextureBackend,
    },
    blending::{Blending, Equation, Factor},
    context::GraphicsContext,
    depth_test::DepthWrite,
    pipeline::{Pipeline, TextureBinding},
    pixel::{NormUnsigned, SRGBA8UI},
    render_gate::RenderGate,
    render_state::RenderState,
    scissor::ScissorRegion,
    shader::{Program, ProgramInterface, Uniform},
    shading_gate::ShadingGate,
    tess::{Interleaved, Mode, Tess, TessBuilder, TessView},
    texture::{Dim2, GenMipmaps, MagFilter, Sampler, Texture},
    Semantics, UniformInterface, Vertex,
};

const VERTEX_SRC: &str = include_str!("gui/gui_es300.glslv");
const FRAGMENT_SRC: &str = include_str!("gui/gui_es300.glslf");

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Semantics)]
pub enum VertexSemantics {
    #[sem(name = "a_Pos", repr = "[f32; 2]", wrapper = "VertexPosition")]
    Position,
    #[sem(name = "a_Uv", repr = "[f32; 2]", wrapper = "VertexUv")]
    Uv,
    #[sem(name = "a_Color", repr = "[u8; 4]", wrapper = "VertexColor")]
    Color,
}

#[derive(Clone, Copy, Debug, Vertex, PartialEq)]
#[vertex(sem = "VertexSemantics")]
pub struct Vertex {
    pub position: VertexPosition,
    pub uv: VertexUv,
    pub color: VertexColor,
}

impl Default for Vertex {
    fn default() -> Self {
        Vertex {
            position: VertexPosition::new([0., 0.]),
            uv: VertexUv::new([0., 0.]),
            color: VertexColor::new([255, 255, 255, 255]),
        }
    }
}

#[derive(Debug, UniformInterface)]
pub struct Uniforms {
    #[uniform(unbound, name = "u_TargetSize")]
    pub target_size: Uniform<[f32; 2]>,
    #[uniform(unbound, name = "u_Texture")]
    pub texture: Uniform<TextureBinding<Dim2, NormUnsigned>>,
}

pub trait GuiBackend:
    TessBackend<Vertex, u16, (), Interleaved>
    // + TessBackend<Vertex, u16, Instance, Interleaved>
    + ShaderBackend
    + PipelineBackend<Dim2>
    + FramebufferBackend<Dim2>
    + RenderGateBackend
    + TessGateBackend<Vertex, u16, (), Interleaved>
    // + TessGateBackend<Vertex, u16, Instance, Interleaved>
    + BufferBackend<Matrix4<f32>>
    // + InstanceSliceBackend<Vertex, u16, Instance, Interleaved, Instance>
    + BufferSliceBackend<Matrix4<f32>>
    + PipelineBuffer<Matrix4<f32>>
    + TextureBackend<Dim2, SRGBA8UI>
    + PipelineTexture<Dim2, SRGBA8UI>
    + VertexSlice<Vertex, u16, (), Interleaved, Vertex>
    + IndexSlice<Vertex, u16, (), Interleaved>
{
}

impl<B: ?Sized> GuiBackend for B where
    B: TessBackend<Vertex, u16, (), Interleaved>
        // + TessBackend<Vertex, u16, Instance, Interleaved>
        + ShaderBackend
        + FramebufferBackend<Dim2>
        + PipelineBackend<Dim2>
        + RenderGateBackend
        + TessGateBackend<Vertex, u16, (), Interleaved>
        // + TessGateBackend<Vertex, u16, Instance, Interleaved>
        + BufferBackend<Matrix4<f32>>
        // + InstanceSliceBackend<Vertex, u16, Instance, Interleaved, Instance>
        + BufferSliceBackend<Matrix4<f32>>
        + PipelineBuffer<Matrix4<f32>>
        + TextureBackend<Dim2, SRGBA8UI>
        + PipelineTexture<Dim2, SRGBA8UI>
        + VertexSlice<Vertex, u16, (), Interleaved, Vertex>
        + IndexSlice<Vertex, u16, (), Interleaved>
{
}

pub struct GuiRenderer<B>
where
    B: GuiBackend,
{
    font_texture_version: u64,
    target_size_in_pixels: Vector2<f32>,
    target_size_in_points: Vector2<f32>,
    dpi_scale: f32,
    textures: HashMap<gui::TextureId, Texture<B, Dim2, SRGBA8UI>>,
    tess: Tess<B, Vertex, u16, (), Interleaved>,
    shader: Option<Program<B, VertexSemantics, (), Uniforms>>,
    meshes: Vec<(Rect, Mesh16)>,
}

impl<B> GuiRenderer<B>
where
    B: GuiBackend,
    [f32; 2]: Uniformable<B>,
    TextureBinding<Dim2, NormUnsigned>: Uniformable<B>,
{
    pub fn new(
        ctx: &mut impl GraphicsContext<Backend = B>,
        target_size: Vector2<f32>,
        dpi_scale: f32,
    ) -> Result<Self> {
        let tess = TessBuilder::build(
            TessBuilder::new(ctx)
                .set_vertices(vec![Vertex::default(); 1024])
                .set_indices(vec![0; 1024]),
        )?;
        let shader = ctx
            .new_shader_program()
            .from_strings(VERTEX_SRC, None, None, FRAGMENT_SRC)?
            .ignore_warnings();
        let mut textures = HashMap::new();
        let initial_font_texture = ctx.new_texture(
            [1, 1],
            0,
            Sampler::default(),
            GenMipmaps::No,
            &[[255, 255, 255, 255]],
        )?;
        textures.insert(gui::TextureId::Egui, initial_font_texture);

        Ok(Self {
            font_texture_version: 0,
            target_size_in_pixels: target_size,
            target_size_in_points: target_size / dpi_scale,
            dpi_scale,
            textures,
            tess,
            shader: Some(shader),
            meshes: Vec::new(),
        })
    }

    pub fn update(
        &mut self,
        ctx: &mut impl GraphicsContext<Backend = B>,
        texture: &gui::Texture,
        meshes: Vec<ClippedMesh>,
    ) -> Result<()> {
        self.meshes.clear();

        if texture.version != self.font_texture_version {
            self.rebuild_font_texture(texture)?;
            self.font_texture_version = texture.version;
        }

        for (clip_rect, mesh) in meshes
            .into_iter()
            .flat_map(|gui::ClippedMesh(r, m)| m.split_to_u16().into_iter().map(move |m| (r, m)))
        {
            assert!(mesh.is_valid());

            let vertex_count = mesh.vertices.len();
            let index_count = mesh.indices.len();

            if self.tess.idx_nb() < index_count || self.tess.vert_nb() < vertex_count {
                let new_vertex_count = self.tess.vert_nb().max(vertex_count);
                let new_index_count = self.tess.idx_nb().max(index_count);

                self.tess = TessBuilder::build(
                    TessBuilder::new(ctx)
                        .set_vertices(vec![Vertex::default(); new_vertex_count])
                        .set_indices(vec![0u16; new_index_count])
                        .set_mode(Mode::Triangle),
                )?;
            }

            self.meshes.push((clip_rect, mesh));
        }

        Ok(())
    }

    pub fn rebuild_font_texture(&mut self, egui_tex: &gui::Texture) -> Result<()> {
        let texture = self.textures.get_mut(&gui::TextureId::Egui).unwrap();
        let gamma = 1.0;
        let data = egui_tex
            .srgba_pixels(gamma)
            .map(|p| p.to_array())
            .collect::<Vec<_>>();
        texture.resize(egui_tex.size().map(|i| i as u32), 0, GenMipmaps::No, &data)?;
        Ok(())
    }

    pub fn draw(
        &mut self,
        pipeline: &mut Pipeline<B>,
        shading_gate: &mut ShadingGate<B>,
    ) -> Result<()> {
        let mut shader = self.shader.take().unwrap();
        let meshes = std::mem::take(&mut self.meshes);

        let result = shading_gate.shade(&mut shader, |mut interface, uni, mut render_gate| {
            interface.set(&uni.target_size, self.target_size_in_points.into());

            for (clip_rect, mesh) in &meshes {
                self.draw_mesh(
                    pipeline,
                    &mut interface,
                    uni,
                    &mut render_gate,
                    clip_rect,
                    mesh,
                )?;
            }

            Ok(())
        });

        self.shader = Some(shader);
        self.meshes = meshes;

        result
    }

    // shut up clippy!!!! shadduuuup!!!!!!
    #[allow(clippy::too_many_arguments)]
    fn draw_mesh(
        &mut self,
        pipeline: &mut Pipeline<B>,
        interface: &mut ProgramInterface<B>,
        uni: &Uniforms,
        render_gate: &mut RenderGate<B>,
        clip_rect: &gui::Rect,
        mesh: &gui::paint::Mesh16,
    ) -> Result<()> {
        assert!(mesh.is_valid());

        let vertex_count = mesh.vertices.len();
        let index_count = mesh.indices.len();

        for (dst, src) in self.tess.vertices_mut()?[..vertex_count]
            .iter_mut()
            .zip(&mesh.vertices)
        {
            *dst = Vertex {
                position: VertexPosition::new([src.pos.x, src.pos.y]),
                uv: VertexUv::new([src.uv.x, src.uv.y]),
                color: VertexColor::new(src.color.to_array()),
            };
        }

        self.tess.indices_mut()?[..index_count].copy_from_slice(&mesh.indices);

        let texture = self.textures.get_mut(&mesh.texture_id).unwrap();
        let bound_texture = pipeline.bind_texture(texture)?;
        interface.set(&uni.texture, bound_texture.binding());

        let width_in_pixels = self.target_size_in_pixels.x;
        let height_in_pixels = self.target_size_in_pixels.y;

        // From https://github.com/emilk/egui/blob/master/egui_glium/src/painter.rs#L233

        // Transform clip rect to physical pixels:
        let clip_min_x = clip_rect.min.x * self.dpi_scale;
        let clip_min_y = clip_rect.min.y * self.dpi_scale;
        let clip_max_x = clip_rect.max.x * self.dpi_scale;
        let clip_max_y = clip_rect.max.y * self.dpi_scale;

        // Make sure clip rect can fit withing an `u32`:
        let clip_min_x = clip_min_x.clamp(0.0, width_in_pixels as f32);
        let clip_min_y = clip_min_y.clamp(0.0, height_in_pixels as f32);
        let clip_max_x = clip_max_x.clamp(clip_min_x, width_in_pixels as f32);
        let clip_max_y = clip_max_y.clamp(clip_min_y, height_in_pixels as f32);

        let clip_min_x = clip_min_x.round() as u32;
        let clip_min_y = clip_min_y.round() as u32;
        let clip_max_x = clip_max_x.round() as u32;
        let clip_max_y = clip_max_y.round() as u32;

        render_gate.render(
            &RenderState::default()
                .set_scissor(ScissorRegion {
                    x: clip_min_x,
                    y: clip_min_y,
                    width: clip_max_x - clip_min_x,
                    height: clip_max_y - clip_min_y,
                })
                .set_blending_separate(
                    Blending {
                        equation: Equation::Additive,
                        src: Factor::One,
                        dst: Factor::SrcAlphaComplement,
                    },
                    Blending {
                        equation: Equation::Additive,
                        src: Factor::DstAlphaComplement,
                        dst: Factor::One,
                    },
                )
                .set_depth_test(None)
                // .set_depth_write(DepthWrite::Off)
                .set_face_culling(None),
            |mut tess_gate| tess_gate.render(TessView::sub(&self.tess, index_count)?),
        )
    }
}
