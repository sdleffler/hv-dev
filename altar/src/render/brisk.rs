use crate::render::*;
use luminance::backend::tess::InstanceSlice;
use luminance::{
    backend::{
        framebuffer::Framebuffer as FramebufferBackend,
        pipeline::{Pipeline as PipelineBackend, PipelineTexture},
        render_gate::RenderGate as RenderGateBackend,
        shader::{Shader as ShaderBackend, Uniformable},
        tess::Tess as TessBackend,
        tess_gate::TessGate as TessGateBackend,
        texture::Texture as TextureBackend,
    },
    blending::{Blending, Equation, Factor},
    context::GraphicsContext,
    depth_stencil::Comparison,
    pipeline::{Pipeline, TextureBinding},
    pixel::{NormUnsigned, SRGBA8UI},
    render_state::RenderState,
    shader::{types::Mat44, Program, Uniform},
    shading_gate::ShadingGate,
    tess::{Interleaved, Mode, Tess, TessBuilder},
    texture::Dim2,
    texture::TexelUpload,
    texture::Texture,
    Semantics, UniformInterface, Vertex,
};
use static_rc::StaticRc;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::hash::Hash;
use std::io::BufReader;
use std::ops::Index;
use std::path::Path;

const VERTEX_SRC: &str = include_str!("brisk/brisk_es300.glslv");
const FRAGMENT_SRC: &str = include_str!("brisk/brisk_es300.glslf");

type Full<T> = StaticRc<T, 2, 2>;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Semantics)]
pub enum VertexSemantics {
    #[sem(name = "i_TCol1", repr = "[f32; 4]", wrapper = "VertexInstanceTCol1")]
    TCol1,
    #[sem(name = "i_TCol2", repr = "[f32; 4]", wrapper = "VertexInstanceTCol2")]
    TCol2,
    #[sem(name = "i_TCol3", repr = "[f32; 4]", wrapper = "VertexInstanceTCol3")]
    TCol3,
    #[sem(name = "i_TCol4", repr = "[f32; 4]", wrapper = "VertexInstanceTCol4")]
    TCol4,
    #[sem(name = "i_Uvs", repr = "[f32; 4]", wrapper = "VertexInstanceUvs")]
    Uvs,
    #[sem(name = "i_Opacity", repr = "f32", wrapper = "VertexInstanceOpacity")]
    Opacity,
    #[sem(name = "i_Dims", repr = "[u32; 2]", wrapper = "VertexInstanceDims")]
    Dims,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Vertex)]
#[vertex(sem = "VertexSemantics", instanced = "true")]
pub struct Instance {
    col1: VertexInstanceTCol1,
    col2: VertexInstanceTCol2,
    col3: VertexInstanceTCol3,
    col4: VertexInstanceTCol4,
    uvs: VertexInstanceUvs,
    opacity: VertexInstanceOpacity,
    dims: VertexInstanceDims,
}

impl Default for Instance {
    fn default() -> Self {
        Instance {
            col1: VertexInstanceTCol1::new([0., 0., 0., 0.]),
            col2: VertexInstanceTCol2::new([0., 0., 0., 0.]),
            col3: VertexInstanceTCol3::new([0., 0., 0., 0.]),
            col4: VertexInstanceTCol4::new([0., 0., 0., 0.]),
            uvs: VertexInstanceUvs::new([0., 0., 0., 0.]),
            opacity: VertexInstanceOpacity::new(0.),
            dims: VertexInstanceDims::new([0, 0]),
        }
    }
}

#[derive(Debug, UniformInterface)]
pub struct Uniforms {
    #[uniform(unbound, name = "u_Texture")]
    texture: Uniform<TextureBinding<Dim2, NormUnsigned>>,
    #[uniform(unbound, name = "u_Projection")]
    projection_matrix: Uniform<Mat44<f32>>,
}

pub trait BriskBackend:
    TessBackend<(), u16, Instance, Interleaved>
    + ShaderBackend
    + PipelineBackend<Dim2>
    + FramebufferBackend<Dim2>
    + RenderGateBackend
    + TessGateBackend<(), u16, Instance, Interleaved>
    + TextureBackend<Dim2, SRGBA8UI>
    + PipelineTexture<Dim2, SRGBA8UI>
    + for<'a> InstanceSlice<'a, (), u16, Instance, Interleaved, Instance>
    + for<'a> Uniformable<'a, Mat44<f32>, Target = Mat44<f32>>
    + for<'a> Uniformable<
        'a,
        TextureBinding<Dim2, NormUnsigned>,
        Target = TextureBinding<Dim2, NormUnsigned>,
    > + for<'a> Uniformable<'a, luminance::shader::types::Mat44<f32>>
{
}

impl<B: ?Sized> BriskBackend for B where
    B: TessBackend<(), u16, Instance, Interleaved>
        + ShaderBackend
        + FramebufferBackend<Dim2>
        + PipelineBackend<Dim2>
        + RenderGateBackend
        + TessGateBackend<(), u16, Instance, Interleaved>
        + TextureBackend<Dim2, SRGBA8UI>
        + PipelineTexture<Dim2, SRGBA8UI>
        + for<'a> InstanceSlice<'a, (), u16, Instance, Interleaved, Instance>
        + for<'a> Uniformable<'a, Mat44<f32>, Target = Mat44<f32>>
        + for<'a> Uniformable<
            'a,
            TextureBinding<Dim2, NormUnsigned>,
            Target = TextureBinding<Dim2, NormUnsigned>,
        > + for<'a> Uniformable<'a, luminance::shader::types::Mat44<f32>>
{
}

#[derive(Default)]
pub struct SpriteBundle(HashMap<SpritesheetId, Vec<(Sprite, Matrix4<f32>)>>);

impl SpriteBundle {
    pub fn clear(&mut self) {
        for (_, instances) in self.0.iter_mut() {
            instances.clear();
        }
    }

    pub fn insert(&mut self, sprite: Sprite, transform: Matrix4<f32>) {
        match self.0.entry(sprite.spritesheet_id) {
            Entry::Occupied(o) => {
                o.into_mut().push((sprite, transform));
            }
            Entry::Vacant(v) => {
                v.insert(vec![(sprite, transform)]);
            }
        }
    }

    pub fn get_sprites_in_spritesheet(
        &self,
        ss_id: SpritesheetId,
    ) -> Option<&Vec<(Sprite, Matrix4<f32>)>> {
        self.0.get(&ss_id)
    }

    pub fn iter_bundle(
        &self,
    ) -> impl Iterator<Item = (&SpritesheetId, &Vec<(Sprite, Matrix4<f32>)>)> {
        self.0.iter()
    }

    pub fn iter_mut_bundle(
        &mut self,
    ) -> impl Iterator<Item = (&SpritesheetId, &mut Vec<(Sprite, Matrix4<f32>)>)> {
        self.0.iter_mut()
    }
}

// Unregistered sprites will be `None` for `id`
#[derive(Debug, Clone)]
pub struct Sprite {
    pub spritesheet_id: SpritesheetId,
    pub flipx: bool,
    pub flipy: bool,
    pub opacity: f32,
    pub offx: i32,
    pub offy: i32,
    pub scale: f32,
    pub frame_id: usize,
    pub width: u32,
    pub height: u32,
}

impl Sprite {
    // Returns the changed id
    pub fn change_spritesheets(&mut self, new_id: SpritesheetId) -> SpritesheetId {
        let tmp = self.spritesheet_id;
        self.spritesheet_id = new_id;
        tmp
    }

    pub fn swap_spritesheets(&mut self, other: &mut Sprite) {
        std::mem::swap(&mut self.spritesheet_id, &mut other.spritesheet_id);
    }

    pub fn set_visibility(&mut self, visible: bool) {
        if visible {
            self.opacity = 1.;
        } else {
            self.opacity = 0.;
        }
    }
}

#[derive(Debug)]
pub struct Spritesheet {
    path: StaticRc<str, 1, 2>,
    id: Option<SpritesheetId>,
    uvs: Vec<F32Box2>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct SpritesheetId(thunderdome::Index);

#[derive(Debug)]
pub struct Spritesheets {
    path_map: HashMap<StaticRc<str, 1, 2>, SpritesheetId>,
    ss_arena: thunderdome::Arena<Spritesheet>,
}

impl Index<SpritesheetId> for Spritesheets {
    type Output = Spritesheet;
    fn index(&self, ss_id: SpritesheetId) -> &Self::Output {
        &self.ss_arena[ss_id.0]
    }
}

impl Default for Spritesheets {
    fn default() -> Self {
        Self {
            path_map: HashMap::new(),
            ss_arena: thunderdome::Arena::new(),
        }
    }
}

impl Drop for Spritesheets {
    fn drop(&mut self) {
        for (_, ss) in self.ss_arena.drain() {
            let (ss_path_half, _) = self.path_map.remove_entry(&ss.path).unwrap();
            // Need to explicitly drop the joined string or static_rc will panik
            drop(Full::join(ss_path_half, ss.path))
        }
        assert!(self.path_map.is_empty());
    }
}

impl Spritesheets {
    // Returns Some(ssid) if the given path was already loaded
    pub fn load_spritesheet(
        &mut self,
        path: &str,
        uvs: Vec<F32Box2>,
    ) -> (SpritesheetId, Option<SpritesheetId>) {
        let full: StaticRc<str, 2, 2> = path.into();
        let (half_1, half_2) = Full::split::<1, 1>(full);
        let arena_idx = self.ss_arena.insert(Spritesheet {
            path: half_1,
            id: None,
            uvs,
        });
        self.ss_arena.get_mut(arena_idx).unwrap().id = Some(SpritesheetId(arena_idx));
        (
            SpritesheetId(arena_idx),
            self.path_map.insert(half_2, SpritesheetId(arena_idx)),
        )
    }

    pub fn get_spritesheet_id(&self, path: &str) -> Option<SpritesheetId> {
        self.path_map.get(path).cloned()
    }

    pub fn get_spritesheet(&self, id: SpritesheetId) -> &Spritesheet {
        self.ss_arena.get(id.0).unwrap()
    }

    pub fn iter_sheets(&self) -> impl Iterator<Item = (SpritesheetId, &Spritesheet)> {
        self.ss_arena.iter().map(|i| (SpritesheetId(i.0), i.1))
    }
}

pub struct SpriteRenderData<B>
where
    B: BriskBackend,
{
    texture: Texture<B, Dim2, SRGBA8UI>,
    tess: Tess<B, (), u16, Instance, Interleaved>,
}

impl<B> SpriteRenderData<B>
where
    B: BriskBackend,
{
    fn from_spritesheet(
        ctxt: &mut impl GraphicsContext<Backend = B>,
        fs: &mut hv::fs::Filesystem,
        ss: &Spritesheet,
    ) -> Result<Self> {
        let spritesheet_img = fs.open(&mut Path::new(&("/".to_owned() + &ss.path)))?;

        let img = image::load(BufReader::new(spritesheet_img), image::ImageFormat::Png)
            .map(|img| img.flipv().to_rgba8())?;
        let (width, height) = img.dimensions();
        let texels = img.as_raw();

        Ok(SpriteRenderData {
            texture: Texture::new_raw(
                ctxt,
                [width, height],
                nearest_sampler(),
                TexelUpload::base_level_without_mipmaps(texels),
            )?,
            tess: TessBuilder::build(
                TessBuilder::new(ctxt)
                    .set_render_vertex_nb(4)
                    .set_mode(Mode::TriangleFan)
                    .set_instances(vec![Instance::default()]),
            )?,
        })
    }
}

pub struct SpriteRenderer<B>
where
    B: BriskBackend,
{
    sprite_cache: HashMap<SpritesheetId, SpriteRenderData<B>>,
    shader: Program<B, VertexSemantics, (), Uniforms>,
}

impl<B> SpriteRenderer<B>
where
    B: BriskBackend,
{
    pub fn new(ctx: &mut impl GraphicsContext<Backend = B>) -> Result<Self> {
        Ok(SpriteRenderer {
            sprite_cache: HashMap::new(),
            shader: ctx
                .new_shader_program::<VertexSemantics, (), Uniforms>()
                .from_strings(VERTEX_SRC, None, None, FRAGMENT_SRC)?
                .ignore_warnings(),
        })
    }

    pub fn load_spritesheets(
        &mut self,
        ctxt: &mut impl GraphicsContext<Backend = B>,
        spritesheets: &Spritesheets,
        fs: &mut hv::fs::Filesystem,
    ) -> Result<()> {
        for (_, sprite_sheet) in spritesheets.iter_sheets() {
            self.load_spritesheet(ctxt, sprite_sheet, fs)?;
        }
        Ok(())
    }

    pub fn load_spritesheet(
        &mut self,
        ctxt: &mut impl GraphicsContext<Backend = B>,
        spritesheet: &Spritesheet,
        fs: &mut hv::fs::Filesystem,
    ) -> Result<()> {
        let ss_id = spritesheet.id.unwrap();
        if let std::collections::hash_map::Entry::Vacant(e) = self.sprite_cache.entry(ss_id) {
            e.insert(SpriteRenderData::from_spritesheet(ctxt, fs, spritesheet)?);
        }
        Ok(())
    }

    fn initialize_instances(
        spritesheet: &Spritesheet,
        sprite_data: &[(Sprite, Matrix4<f32>)],
        mut set_function: impl FnMut(usize, Instance),
    ) -> Result<()> {
        for (i, (sprite, transform)) in sprite_data.iter().enumerate() {
            let (mut uv_bot_left, _, _, mut uv_top_right) = spritesheet
                .uvs
                .get(sprite.frame_id)
                // TODO: in the event of an out of bound this should log a warning and do what
                // with the UVs?
                .ok_or_else(|| {
                    anyhow!(
                        "Out of index when trying to get frame {} from spritesheet {:?}",
                        sprite.frame_id,
                        spritesheet
                    )
                })?
                .corners();

            // Flip sprite by flipping UVs
            if sprite.flipx {
                std::mem::swap(&mut uv_bot_left[0], &mut uv_top_right[0]);
            }

            if sprite.flipy {
                std::mem::swap(&mut uv_bot_left[1], &mut uv_top_right[1]);
            }

            let instance = Instance {
                col1: VertexInstanceTCol1::new([
                    transform.m11,
                    transform.m21,
                    transform.m31,
                    transform.m41,
                ]),
                col2: VertexInstanceTCol2::new([
                    transform.m12,
                    transform.m22,
                    transform.m32,
                    transform.m42,
                ]),
                col3: VertexInstanceTCol3::new([
                    transform.m13,
                    transform.m23,
                    transform.m33,
                    transform.m43,
                ]),
                col4: VertexInstanceTCol4::new([
                    transform.m14,
                    transform.m24,
                    transform.m34,
                    transform.m44,
                ]),
                uvs: VertexInstanceUvs::new([
                    uv_bot_left[0],
                    uv_bot_left[1],
                    uv_top_right[0],
                    uv_top_right[1],
                ]),
                opacity: VertexInstanceOpacity::new(sprite.opacity),
                dims: VertexInstanceDims::new([sprite.width, sprite.height]),
            };

            set_function(i, instance);
        }
        Ok(())
    }

    pub fn upload_sprites(
        &mut self,
        ctxt: &mut impl GraphicsContext<Backend = B>,
        ssid: SpritesheetId,
        sprite_data: &Vec<(Sprite, Matrix4<f32>)>,
        spritesheet: &Spritesheet,
    ) -> Result<()> {
        let render_data = match self.sprite_cache.entry(ssid) {
            Entry::Occupied(o) => &mut *o.into_mut(),
            // TODO: This needs to be set to some default unfound texture!
            Entry::Vacant(_) => {
                return Err(anyhow!(
                    "No loaded render data for sprite sheet {:?}",
                    spritesheet
                ));
            }
        };

        // If the existing tess doesn't have enough memory for all the sprite instances,
        // allocate a new vector and fill it with the instances, then make a new tess
        if render_data.tess.inst_nb() < sprite_data.len() {
            let mut instance_vec = Vec::with_capacity(sprite_data.len().next_power_of_two());
            SpriteRenderer::<B>::initialize_instances(spritesheet, sprite_data, |_, instance| {
                instance_vec.push(instance)
            })?;
            render_data.tess = TessBuilder::build(
                TessBuilder::new(ctxt)
                    .set_render_vertex_nb(4)
                    .set_mode(Mode::TriangleFan)
                    .set_instances(instance_vec),
            )?;
        // Otherwise just overwite the old instances
        } else {
            let mut instances_mut = render_data.tess.instances_mut()?;
            SpriteRenderer::<B>::initialize_instances(spritesheet, sprite_data, |i, instance| {
                instances_mut[i] = instance;
            })?;
        }
        Ok(())
    }

    pub fn draw_sprites(
        &mut self,
        pipeline: &mut Pipeline<B>,
        shading_gate: &mut ShadingGate<B>,
        comparison: Comparison,
        ssid: SpritesheetId,
        spritesheet: &Spritesheet,
        proj: Matrix4<f32>,
    ) -> Result<()> {
        let render_data = match self.sprite_cache.entry(ssid) {
            Entry::Occupied(o) => &mut *o.into_mut(),
            // TODO: This needs to be set to some default unfound texture!
            Entry::Vacant(_) => {
                return Err(anyhow!(
                    "No loaded render data for sprite sheet {:?}",
                    spritesheet
                ));
            }
        };

        shading_gate.shade(
            &mut self.shader,
            |mut interface, uni, mut render_gate| -> Result<()> {
                let bound_texture = pipeline.bind_texture(&mut render_data.texture)?.binding();

                interface.set(&uni.texture, bound_texture);
                interface.set(&uni.projection_matrix, Mat44(proj.into()));

                render_gate.render(
                    &RenderState::default()
                        .set_blending(Blending {
                            equation: Equation::Additive,
                            src: Factor::SrcAlpha,
                            dst: Factor::SrcAlphaComplement,
                        })
                        .set_depth_test(comparison),
                    |mut tess_gate| {
                        tess_gate.render::<Error, _, _, _, _, _>(&render_data.tess)?;
                        Ok(())
                    },
                )
            },
        )?;

        Ok(())
    }
}
