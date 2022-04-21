use crate::render::*;
use image::ImageBuffer;
use image::Rgba;
use luminance::{
    backend::{
        framebuffer::Framebuffer as FramebufferBackend,
        pipeline::{Pipeline as PipelineBackend, PipelineTexture},
        render_gate::RenderGate as RenderGateBackend,
        shader::{Shader as ShaderBackend, Uniformable},
        tess::{IndexSlice, Tess as TessBackend, VertexSlice},
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
    texture::Dim2Array,
    texture::TexelUpload,
    texture::Texture,
    Semantics, UniformInterface, Vertex,
};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::io::BufReader;
use std::path::Path;
use tiled::{
    tile_layer::Chunk, tile_layer::TileLayer, Map, TileAddition, TileRemoval, Tileset, CHUNK_SIZE,
    EMPTY_TILE,
};

const VERTEX_SRC: &str = include_str!("terracotta/terracotta_es300.glslv");
const FRAGMENT_SRC: &str = include_str!("terracotta/terracotta_es300.glslf");

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Semantics)]
pub enum VertexSemantics {
    #[sem(name = "v_Pos", repr = "[f32; 3]", wrapper = "VertexPosition")]
    Position,
    #[sem(name = "v_Uv", repr = "[f32; 2]", wrapper = "VertexUv")]
    Uv,
    #[sem(name = "v_Ts", repr = "u8", wrapper = "VertexTileset")]
    Tileset,
}

#[derive(Clone, Copy, Debug, Vertex, PartialEq)]
#[vertex(sem = "VertexSemantics")]
pub struct Vertex {
    pub position: VertexPosition,
    pub uv: VertexUv,
    pub tileset: VertexTileset,
}

impl Default for Vertex {
    fn default() -> Self {
        Vertex {
            position: VertexPosition::new([0., 0., 0.]),
            uv: VertexUv::new([0., 0.]),
            tileset: VertexTileset::new(0),
        }
    }
}

impl Vertex {
    #[allow(clippy::too_many_arguments)]
    fn quad(
        pos_bot_left: [f32; 3],
        pos_bot_right: [f32; 3],
        pos_top_left: [f32; 3],
        pos_top_right: [f32; 3],
        uv_bot_left: [f32; 2],
        uv_bot_right: [f32; 2],
        uv_top_left: [f32; 2],
        uv_top_right: [f32; 2],
        tileset_id: u8,
    ) -> [Self; 4] {
        let mut quad: [Self; 4] = [Vertex::default(); 4];
        // bottom left
        quad[0] = Vertex::new(
            VertexPosition::new(pos_bot_left),
            VertexUv::new(uv_bot_left),
            VertexTileset::new(tileset_id),
        );
        // bottom right
        quad[1] = Vertex::new(
            VertexPosition::new(pos_bot_right),
            VertexUv::new(uv_bot_right),
            VertexTileset::new(tileset_id),
        );
        // top left
        quad[2] = Vertex::new(
            VertexPosition::new(pos_top_left),
            VertexUv::new(uv_top_left),
            VertexTileset::new(tileset_id),
        );
        // top right
        quad[3] = Vertex::new(
            VertexPosition::new(pos_top_right),
            VertexUv::new(uv_top_right),
            VertexTileset::new(tileset_id),
        );
        quad
    }

    fn oob_vertex() -> Self {
        Vertex {
            position: VertexPosition::new([f32::INFINITY, f32::INFINITY, f32::INFINITY]),
            ..Default::default()
        }
    }
}

#[derive(Debug, UniformInterface)]
pub struct Uniforms {
    #[uniform(unbound, name = "u_Textures")]
    textures: Uniform<TextureBinding<Dim2Array, NormUnsigned>>,
    #[uniform(unbound, name = "u_Transform")]
    transform: Uniform<Mat44<f32>>,
}

pub trait TiledBackend:
    TessBackend<Vertex, u16, (), Interleaved>
    + ShaderBackend
    + PipelineBackend<Dim2>
    + FramebufferBackend<Dim2>
    + RenderGateBackend
    + TessGateBackend<Vertex, u16, (), Interleaved>
    + TextureBackend<Dim2Array, SRGBA8UI>
    + PipelineTexture<Dim2Array, SRGBA8UI>
    + for<'a> VertexSlice<'a, Vertex, u16, (), Interleaved, Vertex>
    + for<'a> IndexSlice<'a, Vertex, u16, (), Interleaved>
    + for<'a> Uniformable<'a, Mat44<f32>, Target = Mat44<f32>>
    + for<'a> Uniformable<
        'a,
        TextureBinding<Dim2Array, NormUnsigned>,
        Target = TextureBinding<Dim2Array, NormUnsigned>,
    >
{
}

impl<B: ?Sized> TiledBackend for B where
    B: TessBackend<Vertex, u16, (), Interleaved>
        + ShaderBackend
        + FramebufferBackend<Dim2>
        + PipelineBackend<Dim2>
        + RenderGateBackend
        + TessGateBackend<Vertex, u16, (), Interleaved>
        + TextureBackend<Dim2Array, SRGBA8UI>
        + PipelineTexture<Dim2Array, SRGBA8UI>
        + for<'a> VertexSlice<'a, Vertex, u16, (), Interleaved, Vertex>
        + for<'a> IndexSlice<'a, Vertex, u16, (), Interleaved>
        + for<'a> Uniformable<'a, Mat44<f32>, Target = Mat44<f32>>
        + for<'a> Uniformable<
            'a,
            TextureBinding<Dim2, NormUnsigned>,
            Target = TextureBinding<Dim2, NormUnsigned>,
        > + for<'a> Uniformable<
            'a,
            TextureBinding<Dim2Array, NormUnsigned>,
            Target = TextureBinding<Dim2Array, NormUnsigned>,
        >
{
}

struct TilesetRenderData {
    tile_pixel_coords: Vec<F32Box2>,
    tile_width: u32,
    tile_height: u32,
    pixel_buffer: ImageBuffer<Rgba<u8>, Vec<u8>>,
}

impl TilesetRenderData {
    pub fn new(tileset: &Tileset, fs: &mut hv::fs::Filesystem) -> Result<Self, Error> {
        if tileset.images.len() > 1 {
            return Err(anyhow!(
                "Multiple images per tilesets aren't supported yet. Expected 1 image, got {}",
                tileset.images.len()
            ));
        }

        // TODO: this can be a box, would be slightly faster and more memory efficient
        let mut tile_pixel_coords = Vec::with_capacity(tileset.tilecount as usize);

        let tileset_img = fs.open(&mut Path::new(
            &("/".to_owned() + &tileset.images[0].source),
        ))?;

        let img = image::load(BufReader::new(tileset_img), image::ImageFormat::Png)
            .map(|img| img.flipv().to_rgba8())?;

        let rows = tileset.tilecount / tileset.columns;
        let top = (rows * (tileset.spacing + tileset.tile_height)) + tileset.margin;
        for row in 1..=rows {
            for column in 0..tileset.columns {
                tile_pixel_coords.push(F32Box2::new(
                    (tileset.margin + ((column * tileset.tile_width) + column * tileset.spacing))
                        as f32,
                    (tileset.spacing
                        + (top
                            - (tileset.margin
                                + ((row * tileset.tile_height) + row * tileset.spacing))))
                        as f32,
                    tileset.tile_width as f32,
                    tileset.tile_height as f32,
                ));
            }
        }

        Ok(TilesetRenderData {
            tile_width: tileset.tile_width,
            tile_height: tileset.tile_height,
            pixel_buffer: img,
            tile_pixel_coords,
        })
    }
}

pub struct ChunkMesh<B>
where
    B: TiledBackend,
{
    dirty: bool,
    tess: Tess<B, Vertex, u16, (), Interleaved>,
}

pub struct TiledRenderer<B>
where
    B: TiledBackend,
{
    // ONLY INDEX THIS VECTOR WITH TILE LAYER ID LLID YOU DINGUS
    chunk_meshes: Vec<HashMap<(i32, i32), ChunkMesh<B>>>,
    tileset_render_cache: HashMap<TilesetType, TilesetRenderData>,
    current_uvs: Vec<F32Box2>,
    current_texture: Texture<B, Dim2Array, SRGBA8UI>,
    tileset_tile_dims: Vec<(f32, f32)>,
    shader: Program<B, VertexSemantics, (), Uniforms>,
    embedded_counter: usize,
}

#[derive(Debug, PartialEq, Eq, Hash)]
enum TilesetType {
    Embedded(usize),
    FilePath(String),
}

impl<B> TiledRenderer<B>
where
    B: TiledBackend,
{
    pub fn new(ctx: &mut impl GraphicsContext<Backend = B>) -> Result<Self> {
        let program = ctx
            .new_shader_program::<VertexSemantics, (), Uniforms>()
            .from_strings(VERTEX_SRC, None, None, FRAGMENT_SRC)?
            .ignore_warnings();
        Ok(Self {
            shader: program,
            chunk_meshes: Vec::new(),
            tileset_render_cache: HashMap::new(),
            current_uvs: Vec::new(),
            current_texture: Texture::new(
                ctx,
                ([1, 1], 1),
                nearest_sampler(),
                TexelUpload::base_level_without_mipmaps(&[[255, 255, 255, 255]]),
            )?,
            tileset_tile_dims: Vec::new(),
            embedded_counter: 0,
        })
    }

    pub fn load_new_map(
        &mut self,
        map: &Map,
        ctx: &mut impl GraphicsContext<Backend = B>,
        fs: &mut hv::fs::Filesystem,
    ) -> Result<()> {
        // Maybe allow the user to change how the cache is inserted and searched? Filename vs tileset name, just filename or full path, etc

        // Clear the current map UVs (TODO: this can be optimized, check first to see if this needs to be cleared by checking which tileset
        // the new map uses and see if the UVs can be reused)
        self.current_uvs.clear();

        // Clear the dimensions from the previous tilesets
        self.tileset_tile_dims.clear();

        // Mark all chunks loaded from last map as dirty
        for entry in self.chunk_meshes.iter_mut() {
            for (_, v) in entry.iter_mut() {
                v.dirty = true;
            }
        }

        let mut max_tileset_width = 0;
        let mut max_tileset_height = 0;

        let mut render_data_keys = Vec::new();

        for tileset in map.tilesets.iter_tilesets() {
            // If there's a filename and it's in the cache, use the precalculated render data. If there's a filename but the render data
            // isn't there, insert it. Otherwise just make new render data, which will get thrown away at the end of this function

            let (key, ts_render_data) = match &tileset.filename {
                Some(f) => match self
                    .tileset_render_cache
                    .entry(TilesetType::FilePath(f.to_string()))
                {
                    Entry::Occupied(o) => (TilesetType::FilePath(f.to_string()), &*o.into_mut()),
                    Entry::Vacant(v) => (
                        TilesetType::FilePath(f.to_string()),
                        &*v.insert(TilesetRenderData::new(tileset, fs)?),
                    ),
                },
                None => {
                    let ts_render_data = TilesetRenderData::new(tileset, fs)?;
                    self.tileset_render_cache
                        .insert(TilesetType::Embedded(self.embedded_counter), ts_render_data);

                    (
                        TilesetType::Embedded(self.embedded_counter),
                        self.tileset_render_cache
                            .get(&TilesetType::Embedded(self.embedded_counter))
                            .unwrap(),
                    )
                }
            };

            render_data_keys.push(key);

            if max_tileset_width < ts_render_data.pixel_buffer.width() {
                max_tileset_width = ts_render_data.pixel_buffer.width()
            }

            if max_tileset_height < ts_render_data.pixel_buffer.height() {
                max_tileset_height = ts_render_data.pixel_buffer.height();
            }
        }

        let mut texel_upload_rgba = vec![
            0x00;
            max_tileset_height as usize
                * max_tileset_width as usize
                * render_data_keys.len()
                * 4
        ];

        for (tileset_num, key) in render_data_keys.iter().enumerate() {
            let ts_render_data = self.tileset_render_cache.get(key).unwrap();
            let layer_index_offset =
                tileset_num * max_tileset_height as usize * max_tileset_width as usize * 4;
            for coord in ts_render_data.tile_pixel_coords.iter() {
                self.current_uvs
                    .push(coord.inverse_scale(max_tileset_width as f32, max_tileset_height as f32));
            }

            for i in 0..ts_render_data.pixel_buffer.height() {
                let target_start =
                    layer_index_offset + ((i as usize * max_tileset_width as usize) * 4);
                let target_end = layer_index_offset
                    + (((i as usize * max_tileset_width as usize)
                        + ts_render_data.pixel_buffer.width() as usize)
                        * 4);
                let src_start = (i * ts_render_data.pixel_buffer.width()) as usize * 4;
                let src_end = ((i + 1) * ts_render_data.pixel_buffer.width()) as usize * 4;

                texel_upload_rgba[target_start..target_end]
                    .copy_from_slice(&ts_render_data.pixel_buffer.as_raw()[src_start..src_end]);
            }

            self.tileset_tile_dims.push((
                ts_render_data.tile_width as f32,
                ts_render_data.tile_height as f32,
            ));
        }

        self.current_texture.resize_raw(
            (
                [max_tileset_width, max_tileset_height],
                render_data_keys.len().try_into().unwrap(),
            ),
            TexelUpload::base_level_without_mipmaps(&texel_upload_rgba),
        )?;

        if self.chunk_meshes.len() < map.tile_layers.len() {
            self.chunk_meshes.resize_with(
                map.tile_layers.len() - self.chunk_meshes.len(),
                HashMap::new,
            );
        }

        for layer in map.tile_layers.iter() {
            for ((chunk_x, chunk_y), chunk) in layer.data.chunks() {
                let chunk_mesh_map = &mut self.chunk_meshes[layer.id.llid as usize];

                if let Entry::Vacant(v) = chunk_mesh_map.entry((*chunk_x, *chunk_y)) {
                    v.insert(ChunkMesh {
                        tess: TessBuilder::build(
                            TessBuilder::new(ctx)
                                .set_mode(Mode::Triangle)
                                .set_vertices(vec![
                                    Vertex::oob_vertex();
                                    (CHUNK_SIZE * CHUNK_SIZE) as usize * 4
                                ])
                                .set_indices(vec![
                                    u16::MAX;
                                    (CHUNK_SIZE * CHUNK_SIZE) as usize * 6
                                ]),
                        )?,
                        dirty: false,
                    });
                }

                self.fill_chunk_mesh(
                    layer.id.llid as usize,
                    *chunk_x,
                    *chunk_y,
                    // TODO: offset_y vs height?
                    layer.offset_y as f32,
                    chunk,
                    map.meta_data.tilewidth,
                    map.meta_data.tileheight,
                )?;
            }
        }

        Ok(())
    }

    pub fn remove_tiles<'a>(
        &mut self,
        removals: impl Iterator<Item = &'a TileRemoval>,
    ) -> Result<()> {
        let mut dirty_chunks = HashMap::new();

        for removal in removals {
            let key = (removal.chunk_x, removal.chunk_y, removal.layer_id);

            if let std::collections::hash_map::Entry::Vacant(e) = dirty_chunks.entry(key) {
                e.insert(vec![removal.chunk_index]);
            } else {
                dirty_chunks
                    .get_mut(&key)
                    .unwrap()
                    .push(removal.chunk_index);
            }
        }

        for ((chunk_x, chunk_y, layer_id), chunk_indices) in dirty_chunks.iter() {
            let chunk_mesh = self.chunk_meshes[layer_id.llid as usize]
                .get_mut(&(*chunk_x, *chunk_y))
                .ok_or_else(|| anyhow!("No such chunk {:?}", (chunk_x, chunk_y)))?;

            let mut ibo_ref = chunk_mesh.tess.indices_mut()?;

            for chunk_index in chunk_indices.iter() {
                ibo_ref[(chunk_index * 6)..(chunk_index * 6) + 6].copy_from_slice(&[u16::MAX; 6]);
            }
        }
        Ok(())
    }

    pub fn add_tiles<'a>(
        &mut self,
        additions: impl Iterator<Item = &'a TileAddition>,
        map: &Map,
    ) -> Result<()> {
        let mut dirty_chunks = HashMap::new();

        for addition in additions {
            let key = (addition.chunk_x, addition.chunk_y, addition.layer_id);
            if let std::collections::hash_map::Entry::Vacant(e) = dirty_chunks.entry(key) {
                e.insert(vec![(addition.chunk_index, addition.new_id)]);
            } else {
                dirty_chunks
                    .get_mut(&key)
                    .unwrap()
                    .push((addition.chunk_index, addition.new_id));
            }
        }

        for ((chunk_x, chunk_y, layer_id), chunk_idxs_and_tiles) in dirty_chunks.iter() {
            let chunk_mesh = self.chunk_meshes[layer_id.llid as usize]
                .get_mut(&(*chunk_x, *chunk_y))
                .ok_or_else(|| {
                    anyhow!(
                        "No support for adding new chunks yet {} {}",
                        chunk_x,
                        chunk_y
                    )
                })?;

            let mut ibo_ref = chunk_mesh.tess.indices_mut()?;

            for (chunk_index, _) in chunk_idxs_and_tiles.iter() {
                let ibo_i: u16 = (*chunk_index).try_into().unwrap();

                ibo_ref[(chunk_index * 6)..(chunk_index * 6) + 6].copy_from_slice(&[
                    ibo_i * 4,
                    ibo_i * 4 + 1,
                    ibo_i * 4 + 2,
                    ibo_i * 4 + 1,
                    ibo_i * 4 + 2,
                    ibo_i * 4 + 3,
                ]);
            }

            drop(ibo_ref);

            let mut vbo_ref = chunk_mesh.tess.vertices_mut()?;

            // TODO: a lot of this code is duplicated from the fill_mesh function, refactor this
            for (i, new_id) in chunk_idxs_and_tiles.iter() {
                let tileset_id = new_id.tileset_id();

                let uv_box = self.current_uvs[(new_id.gid() - 1) as usize];

                assert!(uv_box.is_valid());
                let (bot_left, bot_right, top_left, top_right) = uv_box.corners();

                let bottom_left_x = (((chunk_x * CHUNK_SIZE as i32)
                    + (*i as i32 % CHUNK_SIZE as i32)) as f32)
                    * map.meta_data.tilewidth as f32;
                let bottom_left_y = (((chunk_y * CHUNK_SIZE as i32)
                    + (*i as i32 / CHUNK_SIZE as i32)) as f32)
                    * map.meta_data.tileheight as f32;

                let (tileset_tile_width, tileset_tile_height) =
                    self.tileset_tile_dims[tileset_id as usize];

                let z_offset = map.tile_layers[layer_id.llid as usize].offset_y as f32;

                vbo_ref[i * 4..(i * 4) + 4].copy_from_slice(&Vertex::quad(
                    [bottom_left_x, bottom_left_y, z_offset],
                    [bottom_left_x + tileset_tile_width, bottom_left_y, z_offset],
                    [bottom_left_x, bottom_left_y - tileset_tile_height, z_offset],
                    [
                        bottom_left_x + tileset_tile_width,
                        bottom_left_y - tileset_tile_height,
                        z_offset,
                    ],
                    bot_left,
                    bot_right,
                    top_left,
                    top_right,
                    tileset_id,
                ));
            }
        }
        Ok(())
    }

    pub fn draw(
        &mut self,
        transform: Matrix4<f32>,
        pipeline: &mut Pipeline<B>,
        shading_gate: &mut ShadingGate<B>,
        comparison: Comparison,
        map: &Map,
    ) -> Result<()> {
        for tile_layer in map.tile_layers.iter() {
            self.draw_chunks(
                transform,
                pipeline,
                shading_gate,
                comparison,
                tile_layer,
                tile_layer.data.chunk_coordinates().copied(),
            )?;
        }
        Ok(())
    }

    pub fn draw_layer(
        &mut self,
        transform: Matrix4<f32>,
        pipeline: &mut Pipeline<B>,
        shading_gate: &mut ShadingGate<B>,
        comparison: Comparison,
        tile_layer: &TileLayer,
    ) -> Result<()> {
        self.draw_chunks(
            transform,
            pipeline,
            shading_gate,
            comparison,
            tile_layer,
            tile_layer.data.chunk_coordinates().copied(),
        )
    }

    pub fn draw_chunks(
        &mut self,
        transform: Matrix4<f32>,
        pipeline: &mut Pipeline<B>,
        shading_gate: &mut ShadingGate<B>,
        comparison: Comparison,
        tile_layer: &TileLayer,
        coords: impl IntoIterator<Item = (i32, i32)>,
    ) -> Result<()> {
        for coord in coords {
            self.draw_chunk(
                transform,
                pipeline,
                shading_gate,
                comparison,
                tile_layer,
                coord.0,
                coord.1,
            )?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_chunk(
        &mut self,
        transform: Matrix4<f32>,
        pipeline: &mut Pipeline<B>,
        shading_gate: &mut ShadingGate<B>,
        comparison: Comparison,
        tile_layer: &TileLayer,
        chunk_x: i32,
        chunk_y: i32,
    ) -> Result<()> {
        shading_gate.shade(
            &mut self.shader,
            |mut interface, uni, mut render_gate| -> Result<()> {
                let bound_texture = pipeline.bind_texture(&mut self.current_texture)?.binding();

                interface.set(&uni.textures, bound_texture);
                interface.set(&uni.transform, Mat44(transform.into()));

                render_gate.render(
                    &RenderState::default()
                        .set_blending(Blending {
                            equation: Equation::Additive,
                            src: Factor::SrcAlpha,
                            dst: Factor::SrcAlphaComplement,
                        })
                        .set_depth_test(comparison),
                    |mut tess_gate| {
                        if let Some(chunk_mesh) =
                            self.chunk_meshes[tile_layer.id.llid as usize].get(&(chunk_x, chunk_y))
                        {
                            if !chunk_mesh.dirty {
                                tess_gate.render::<Error, _, _, _, _, _>(&chunk_mesh.tess)?;
                            }
                        }
                        Ok(())
                    },
                )
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn fill_chunk_mesh(
        &mut self,
        layer_index: usize,
        chunk_x: i32,
        chunk_y: i32,
        z_offset: f32,
        chunk: &Chunk,
        map_tile_width: u32,
        map_tile_height: u32,
    ) -> Result<()> {
        assert_eq!(CHUNK_SIZE, 16);

        let mesh = self.chunk_meshes[layer_index]
            .get_mut(&(chunk_x, chunk_y))
            .unwrap();

        let mut vbo = mesh.tess.vertices_mut()?;
        // Update the dirty chunk flag, filling in the chunk makes it good to use again
        mesh.dirty = false;

        for (i, tile) in chunk.tiles().iter().enumerate() {
            if *tile != EMPTY_TILE {
                let tileset_id = tile.tileset_id();

                let uv_box = self.current_uvs[(tile.gid() - 1) as usize];

                assert!(uv_box.is_valid());
                let (bot_left, bot_right, top_left, top_right) = uv_box.corners();

                let bottom_left_x = (((chunk_x * CHUNK_SIZE as i32)
                    + (i as i32 % CHUNK_SIZE as i32)) as f32)
                    * map_tile_width as f32;
                let bottom_left_y = (((chunk_y * CHUNK_SIZE as i32)
                    + (i as i32 / CHUNK_SIZE as i32)) as f32)
                    * map_tile_height as f32;

                let (tileset_tile_width, tileset_tile_height) =
                    self.tileset_tile_dims[tileset_id as usize];

                vbo[i * 4..(i * 4) + 4].copy_from_slice(&Vertex::quad(
                    [bottom_left_x, bottom_left_y, z_offset],
                    [bottom_left_x + tileset_tile_width, bottom_left_y, z_offset],
                    [bottom_left_x, bottom_left_y - tileset_tile_height, z_offset],
                    [
                        bottom_left_x + tileset_tile_width,
                        bottom_left_y - tileset_tile_height,
                        z_offset,
                    ],
                    bot_left,
                    bot_right,
                    top_left,
                    top_right,
                    tileset_id,
                ));
            }
        }

        drop(vbo);

        let mut ibo = mesh.tess.indices_mut()?;

        for (i, tile) in chunk.tiles().iter().enumerate() {
            if *tile != EMPTY_TILE {
                let ibo_i: u16 = i.try_into().unwrap();

                ibo[i * 6..(i * 6) + 6].copy_from_slice(&[
                    ibo_i * 4,
                    ibo_i * 4 + 1,
                    ibo_i * 4 + 2,
                    ibo_i * 4 + 1,
                    ibo_i * 4 + 2,
                    ibo_i * 4 + 3,
                ]);
            }
        }

        Ok(())
    }
}
