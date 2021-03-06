pub mod json_parser;
pub mod lua_parser;
pub mod object_layer;
pub mod tile_layer;

use crate::object_layer::*;
use crate::tile_layer::*;
pub use hv::math::Vector2;
use hv::prelude::*;
use serde_json::value::Value;

// use hv_friends::math::Box2;

use std::{collections::HashMap, io::Read, path::Path};

pub const EMPTY_TILE: TileId = TileId(0, TileMetaData(0));
pub const CHUNK_SIZE: u32 = 16;

const FLIPPED_HORIZONTALLY_FLAG: u32 = 0x80000000;
const FLIPPED_VERTICALLY_FLAG: u32 = 0x40000000;
const FLIPPED_DIAGONALLY_FLAG: u32 = 0x20000000;
const UNSET_FLAGS: u32 = 0x1FFFFFFF;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Color {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl LuaUserData for Color {}

impl Color {
    fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a: 1 }
    }

    fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color { r, g, b, a }
    }

    fn from_rgb_u32(c: u32) -> Self {
        Color::from_rgba(
            (c & 0xFF) as u8,
            ((c & 0xFF00) >> 8) as u8,
            ((c & 0xFF0000) >> 16) as u8,
            ((c & 0xFF000000) >> 24) as u8,
        )
    }

    fn from_tiled_hex(hex: &str) -> Result<Color, Error> {
        Ok(Color::from_rgb_u32(u32::from_str_radix(
            hex.trim_start_matches('#'),
            16,
        )?))
    }
}

#[derive(Debug, Clone)]
pub enum LayerType {
    Tile,
    Object,
}

// TODO: This type was pulled from the Tiled crate, but the Color and File variants
// are never constructed. This might be a bug depending on what the "properties"
// table contains
#[derive(Debug, PartialEq, Clone)]
pub enum Property {
    Bool(bool),
    Float(f64),
    Int(i64),
    String(String),
    Obj(ObjectId),
    Color(String),
    File(String),
}

macro_rules! as_rust_type {
    ( $fun_name:ident, $return_type:ty, $error_name: literal, $enum_var:ident ) => {
        pub fn $fun_name(&self) -> Result<$return_type> {
            match self {
                Property::$enum_var(e) => Ok(e),
                p => Err(anyhow!("Attempted to get a {} from a {:?}", $error_name, p)),
            }
        }
    };
}

impl Property {
    as_rust_type!(as_bool, &bool, "bool", Bool);
    as_rust_type!(as_float, &f64, "float", Float);
    as_rust_type!(as_int, &i64, "int", Int);
    as_rust_type!(as_str, &str, "string", String);
    as_rust_type!(as_obj_id, &ObjectId, "object", Obj);
    as_rust_type!(as_file, &str, "file", File);

    pub fn as_color(&self) -> Result<Color> {
        match self {
            Property::Color(c) => Ok(Color::from_tiled_hex(c)?),
            p => Err(anyhow!("Attempted to get a color from a {:?}", p)),
        }
    }

    pub fn from_json_entry(v: &Value) -> Result<Self> {
        match v {
            Value::Bool(b) => Ok(Property::Bool(*b)),
            Value::Number(n) => {
                if n.is_f64() {
                    Ok(Property::Float(n.as_f64().unwrap()))
                } else {
                    Ok(Property::Int(n.as_i64().unwrap()))
                }
            }
            Value::String(s) => Ok(Property::String(s.as_str().to_owned())),
            Value::Object(o) => Ok(Property::Obj(ObjectId::new(
                o.get("id")
                    .ok_or_else(|| anyhow!("All object properties should have IDs"))?
                    .as_u64()
                    .ok_or_else(|| anyhow!("Should be able to turn object ID into u64"))?
                    .try_into()
                    .expect("Object IDs should fit in a u64"),
                false,
            ))),
            v => Err(anyhow!("Not sure what this should be turned into {:?}", v)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Properties(HashMap<String, Property>);

impl Properties {
    pub fn get_property(&self, key: &str) -> Option<&Property> {
        self.0.get(key)
    }
}

#[derive(Debug, Clone)]
pub enum Orientation {
    Orthogonal,
    Isometric,
}

#[derive(Debug, Clone)]
pub enum RenderOrder {
    RightDown,
    RightUp,
    LeftDown,
    LeftUp,
}

bitfield::bitfield! {
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
    pub struct TileMetaData(u32);
    pub flipx,                   _ : 31;
    pub flipy,                   _ : 30;
    pub diag_flip,               _ : 29;
    pub tileset_id, set_tileset_id : 28, 0;
}

impl TileMetaData {
    pub fn new(tileset_id: u8, flipx: bool, flipy: bool, diagonal_flip: bool) -> TileMetaData {
        TileMetaData(
            (flipx as u32) << 31
                | (flipy as u32) << 30
                | (diagonal_flip as u32) << 29
                | tileset_id as u32,
        )
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Copy, Hash)]
pub struct TileId(u32, TileMetaData);

impl TileId {
    pub fn to_index(&self) -> Option<usize> {
        if self.0 == 0 {
            None
        } else {
            Some((self.0 - 1) as usize)
        }
    }

    // Input the tile id here as is found in tiled
    pub fn new(
        tile_id: u32,
        tileset_id: u8,
        flipx: bool,
        flipy: bool,
        diagonal_flip: bool,
    ) -> TileId {
        // If any of the top 3 bits of the tileset_id are stored, panic. We can't have
        // tileset ids that are larger than 29 bits due to the top 3 bits being reserved for
        // flip data
        TileId(
            tile_id + 1,
            TileMetaData::new(tileset_id, flipx, flipy, diagonal_flip),
        )
    }

    pub fn tileset_id(&self) -> u8 {
        self.1.tileset_id() as u8
    }

    pub fn gid(&self) -> u32 {
        self.0
    }

    fn from_gid(mut gid: u32, tile_buffer: &[u8]) -> TileId {
        // For each tile, we check the flip flags and set the metadata with them.
        // We then unset the flip flags in the tile ID
        let flipx = (gid & FLIPPED_HORIZONTALLY_FLAG) != 0;
        let flipy = (gid & FLIPPED_VERTICALLY_FLAG) != 0;
        let diag_flip = (gid & FLIPPED_DIAGONALLY_FLAG) != 0;

        gid &= UNSET_FLAGS;

        let tileset_id = tile_buffer[gid as usize];

        TileId(gid, TileMetaData::new(tileset_id, flipx, flipy, diag_flip))
    }
}

#[derive(Debug, Clone)]
pub struct MapMetaData {
    pub tsx_ver: String,
    pub lua_ver: Option<String>,
    pub tiled_ver: String,
    pub orientation: Orientation,
    pub render_order: RenderOrder,
    pub width: u32,
    pub height: u32,
    pub tilewidth: u32,
    pub tileheight: u32,
    pub nextlayerid: u32,
    pub nextobjectid: u32,
    pub properties: Properties,
}

#[derive(Debug, Clone)]
pub struct TileRemoval {
    _id: TileId,
    pub layer_id: TileLayerId,
    pub chunk_x: i32,
    pub chunk_y: i32,
    pub chunk_index: usize,
}

#[derive(Debug, Clone)]
pub struct TileAddition {
    _changed_id: Option<TileId>,
    pub new_id: TileId,
    pub layer_id: TileLayerId,
    pub chunk_x: i32,
    pub chunk_y: i32,
    pub chunk_index: usize,
}

#[derive(Debug, Clone)]
pub struct ObjectRemoval;

#[derive(Debug, Clone)]
pub struct ObjectAddition;

#[derive(Debug)]
pub struct Map {
    pub meta_data: MapMetaData,
    pub tile_layers: Vec<TileLayer>,
    pub object_layers: Vec<ObjectLayer>,
    pub tilesets: Tilesets,
    tile_layer_map: HashMap<String, TileLayerId>,
    object_layer_map: HashMap<String, ObjectLayerId>,
    obj_slab: slab::Slab<Object>,
    obj_id_to_ref_map: HashMap<ObjectId, ObjectRef>,
    pub tile_additions: shrev::EventChannel<TileAddition>,
    pub tile_removals: shrev::EventChannel<TileRemoval>,
    pub object_additions: shrev::EventChannel<ObjectAddition>,
    pub object_removals: shrev::EventChannel<ObjectRemoval>,
}

impl Clone for Map {
    fn clone(&self) -> Self {
        Map {
            meta_data: self.meta_data.clone(),
            tile_layers: self.tile_layers.clone(),
            object_layers: self.object_layers.clone(),
            tilesets: self.tilesets.clone(),
            tile_layer_map: self.tile_layer_map.clone(),
            object_layer_map: self.object_layer_map.clone(),
            obj_slab: self.obj_slab.clone(),
            obj_id_to_ref_map: self.obj_id_to_ref_map.clone(),
            tile_additions: shrev::EventChannel::new(),
            tile_removals: shrev::EventChannel::new(),
            object_additions: shrev::EventChannel::new(),
            object_removals: shrev::EventChannel::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum CoordSpace {
    Pixel,
    Tile,
}

impl Map {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        meta_data: MapMetaData,
        tile_layers: Vec<TileLayer>,
        object_layers: Vec<ObjectLayer>,
        tilesets: Tilesets,
        tile_layer_map: HashMap<String, TileLayerId>,
        object_layer_map: HashMap<String, ObjectLayerId>,
        obj_slab: slab::Slab<Object>,
        obj_id_to_ref_map: HashMap<ObjectId, ObjectRef>,
    ) -> Self {
        Map {
            meta_data,
            tile_layers,
            object_layers,
            tilesets,
            tile_layer_map,
            object_layer_map,
            obj_slab,
            obj_id_to_ref_map,
            tile_additions: shrev::EventChannel::new(),
            tile_removals: shrev::EventChannel::new(),
            object_additions: shrev::EventChannel::new(),
            object_removals: shrev::EventChannel::new(),
        }
    }

    pub fn remove_tile(
        &mut self,
        x: i32,
        y: i32,
        coordinate_space: CoordSpace,
        layer_id: TileLayerId,
    ) {
        let (x, y) = match coordinate_space {
            CoordSpace::Pixel => (
                x / (self.meta_data.tilewidth) as i32,
                y / (self.meta_data.tileheight as i32),
            ),
            CoordSpace::Tile => (x, y),
        };

        let (chunk_x, chunk_y, tile_x, tile_y) = to_chunk_indices_and_subindices(x, y);
        let chunk_index = (tile_y * CHUNK_SIZE + tile_x) as usize;

        if let Some(tile_id) =
            self.tile_layers[layer_id.llid as usize]
                .data
                .remove_tile(chunk_x, chunk_y, chunk_index)
        {
            assert!(tile_id.to_index().is_some());
            self.tile_removals.single_write(TileRemoval {
                _id: tile_id,
                layer_id,
                chunk_x,
                chunk_y,
                chunk_index,
            })
        } else {
            // TODO: maybe log?
        }
    }

    pub fn set_tile(
        &mut self,
        x: i32,
        y: i32,
        coordinate_space: CoordSpace,
        layer_id: TileLayerId,
        tile: TileId,
    ) {
        let (x, y) = match coordinate_space {
            CoordSpace::Pixel => (
                x / (self.meta_data.tilewidth as i32),
                y / (self.meta_data.tileheight as i32),
            ),
            CoordSpace::Tile => (x, y),
        };

        let layer = &mut self.tile_layers[layer_id.llid as usize];

        let (chunk_x, chunk_y, tile_x, tile_y) = to_chunk_indices_and_subindices(x, y);
        let chunk_index = (tile_y * CHUNK_SIZE + tile_x) as usize;

        let _changed_id = layer.data.set_tile(chunk_x, chunk_y, chunk_index, tile);
        self.tile_additions.single_write(TileAddition {
            new_id: tile,
            _changed_id,
            layer_id,
            chunk_x,
            chunk_y,
            chunk_index,
        });
    }

    pub fn get_tile(
        &self,
        x: i32,
        y: i32,
        layer_id: TileLayerId,
        coordinate_space: CoordSpace,
    ) -> Option<TileId> {
        let (x, y) = match coordinate_space {
            CoordSpace::Pixel => (
                x / (self.meta_data.tilewidth as i32),
                y / (self.meta_data.tileheight as i32),
            ),
            CoordSpace::Tile => (x, y),
        };

        let layer = &self.tile_layers[layer_id.llid as usize];

        match layer.data.get_tile(x, y) {
            Some(t_id) if t_id.to_index().is_some() => Some(t_id),
            Some(_) | None => None,
        }
    }

    pub fn get_tiles_in_bb(
        &self,
        mins: Point2<i32>,
        maxs: Point2<i32>,
        layer_id: TileLayerId,
        coordinate_space: CoordSpace,
    ) -> impl Iterator<Item = (TileId, i32, i32)> + '_ {
        let box_in_tiles = match coordinate_space {
            CoordSpace::Pixel => (
                (
                    (mins.x as f32 / (self.meta_data.tilewidth) as f32).floor() as i32,
                    (mins.y as f32 / (self.meta_data.tileheight) as f32).floor() as i32,
                ),
                (
                    (maxs.x as f32 / (self.meta_data.tilewidth as f32)).ceil() as i32,
                    (maxs.y as f32 / (self.meta_data.tileheight as f32)).ceil() as i32,
                ),
            ),

            CoordSpace::Tile => ((mins.x, mins.y), (maxs.x, maxs.y)),
        };
        ((box_in_tiles.0 .1)..=(box_in_tiles.1 .1)).flat_map(move |y| {
            ((box_in_tiles.0 .0)..=(box_in_tiles.1 .0)).filter_map(move |x| {
                self.get_tile(x, y, layer_id, CoordSpace::Tile)
                    .map(|t| (t, x, y))
            })
        })
    }

    pub fn get_obj_from_ref(&self, obj_ref: &ObjectRef) -> &Object {
        &self.obj_slab[obj_ref.0]
    }

    pub fn get_objs_from_obj_group<'a>(
        &'a self,
        obj_group: &'a ObjectGroup,
    ) -> impl Iterator<Item = &'a Object> + 'a {
        obj_group.get_obj_refs().map(move |o| &self.obj_slab[o.0])
    }

    pub fn get_obj_grp_from_tile_id(&self, tileid: &TileId) -> Option<&ObjectGroup> {
        self.tilesets
            .get_tile(tileid)
            .and_then(|t| t.objectgroup.as_ref())
    }

    pub fn get_obj_grp_from_layer_id(&self, obj_layer_id: &ObjectLayerId) -> &ObjectGroup {
        &self.object_layers[obj_layer_id.llid as usize]
    }

    pub fn get_object_ids_in_object_group_by_name(
        &self,
        obj_layer_id: &ObjectLayerId,
        name: &str,
    ) -> &[ObjectId] {
        self.object_layers[obj_layer_id.llid as usize]
            .object_name_map
            .get(name)
            .map_or(&[], |vec| vec.as_slice())
    }

    pub fn get_object_from_id(&self, obj_id: &ObjectId) -> Option<&Object> {
        self.obj_id_to_ref_map
            .get(obj_id)
            .map(|obj_ref| self.get_obj_from_ref(obj_ref))
    }

    pub fn get_tile_layer_id_by_name(&self, layer_name: &str) -> Option<TileLayerId> {
        self.tile_layer_map.get(layer_name).copied()
    }

    pub fn get_tile_layer_by_name(&self, layer_name: &str) -> Option<&TileLayer> {
        self.tile_layer_map
            .get(layer_name)
            .map(|lid| &self.tile_layers[lid.llid as usize])
    }
}

#[derive(Debug, Clone)]
// The u32 here represents the duration, TileId is which TileId is associated with said duration
pub struct Animation(Vec<(TileId, u32)>);

#[derive(Debug, Clone)]
pub struct Tile {
    pub id: TileId,
    pub tile_type: Option<String>,
    pub probability: f32,
    pub properties: Properties,
    pub objectgroup: Option<ObjectGroup>,
    pub animation: Option<Animation>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Image {
    pub source: String,
    pub width: u32,
    pub height: u32,
    // Note that although this is parsed, it's not actually used lmao TODO
    pub trans_color: Option<Color>,
}

impl Image {
    pub fn from_lua(it: &LuaTable, prefix: Option<&str>) -> Result<Self, Error> {
        Ok(Image {
            source: prefix.unwrap_or("").to_owned() + it.get::<_, LuaString>("image")?.to_str()?,
            width: it.get("imagewidth")?,
            height: it.get("imageheight")?,
            trans_color: match it.get::<_, LuaString>("transparentcolor") {
                Ok(s) => Some(Color::from_tiled_hex(s.to_str()?)?),
                _ => None,
            },
        })
    }

    pub fn from_json(
        v: &serde_json::Map<String, Value>,
        prefix: Option<&str>,
    ) -> Result<Self, Error> {
        Ok(Image {
            source: prefix.unwrap_or("").to_owned() + v.get("image")
                        .ok_or_else(|| anyhow!("Should've gotten an image, if this is a list, image lists aren't supported yet"))?
                        .as_str()
                        .ok_or_else(|| anyhow!("Image value wasn't an image"))?,
            width: v.get("imagewidth").ok_or_else(|| anyhow!("Should've gotten an imagewidth"))?.as_u64().ok_or_else(|| anyhow!("Imagewidth value wasn't a u32"))?.try_into().expect("Check your imagewidth wtf"),
            height: v.get("imageheight").ok_or_else(|| anyhow!("Should've gotten an imageheight"))?.as_u64().ok_or_else(|| anyhow!("Imageheight value wasn't a u32"))?.try_into().expect("Check your imageheight wtf"),
            trans_color: v.get("transparentcolor").map(|s| {
                Color::from_tiled_hex(
                    s.as_str()
                        .ok_or_else(|| anyhow!("Expected a string for transparentcolor"))?,
                )
            })
            .transpose()?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Tileset {
    pub first_gid: u32,
    pub name: String,
    pub tile_width: u32,
    pub tile_height: u32,
    pub spacing: u32,
    pub margin: u32,
    pub tilecount: u32,
    pub columns: u32,
    pub tiles: HashMap<TileId, Tile>,
    pub properties: Properties,
    pub images: Vec<Image>,
    pub filename: Option<String>,
}

impl Tileset {
    fn get_tile(&self, tile_id: &TileId) -> Option<&Tile> {
        self.tiles.get(tile_id)
    }
}

#[derive(Debug, Clone)]
pub struct Tilesets(Vec<Tileset>);

impl Tilesets {
    pub fn get_tile(&self, tile_id: &TileId) -> Option<&Tile> {
        self.0[tile_id.1.tileset_id() as usize].get_tile(tile_id)
    }

    pub fn iter_tilesets(&self) -> std::slice::Iter<'_, Tileset> {
        self.0.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}
