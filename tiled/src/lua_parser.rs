use crate::*;
use hv::prelude::*;

// For some reason, in the lua encoding, text is stored under shape
// Why????? In any case I made this type to store both a text and an
// actual shape object
enum LuaShapeResolution {
    Text(Text),
    ObjectShape(ObjectShape),
}

impl Color {
    fn from_tiled_lua_table(c_t: &LuaTable) -> Result<Color, Error> {
        match c_t.get::<_, LuaTable>("color") {
            Ok(t) => {
                let mut iter = t.sequence_values();
                let r = iter
                    .next()
                    .ok_or_else(|| anyhow!("Should've gotten a value for R, got nothing"))??;
                let g = iter
                    .next()
                    .ok_or_else(|| anyhow!("Should've gotten a value for G, got nothing"))??;
                let b = iter
                    .next()
                    .ok_or_else(|| anyhow!("Should've gotten a value for B, got nothing"))??;
                Ok(Color::from_rgb(r, g, b))
            }
            Err(_) => Ok(Color::from_rgb(0, 0, 0)),
        }
    }
}

fn parse_layer_type(t: &LuaTable) -> Result<LayerType, Error> {
    match t.get::<_, LuaString>("type")?.to_str()? {
        "objectgroup" => Ok(LayerType::Object),
        "tilelayer" => Ok(LayerType::Tile),
        s => Err(anyhow!("Unsupported layer type: {}", s)),
    }
}

fn parse_properties(props: &LuaTable) -> Result<Properties, Error> {
    let mut properties = HashMap::new();
    let props_t = props.get::<_, LuaTable>("properties")?;

    for pair_res in props_t.pairs() {
        let pair = pair_res?;
        let val = match pair.1 {
            LuaValue::Boolean(b) => Property::Bool(b),
            LuaValue::Integer(i) => Property::Int(i),
            LuaValue::Number(n) => Property::Float(n),
            LuaValue::String(s) => Property::String(s.to_str()?.to_owned()),
            LuaValue::Table(t) => Property::Obj(ObjectId::new(t.get("id")?, false)), // I believe tables will only come through for Object properties
            l => {
                return Err(anyhow!(
                    "Got an unexpected value in the properties section: {:?}",
                    l
                ))
            }
        };
        properties.insert(pair.0, val);
    }
    Ok(Properties(properties))
}

fn parse_map_meta_data(map_table: &LuaTable) -> Result<MapMetaData, Error> {
    let render_order = match map_table.get::<_, LuaString>("renderorder")?.to_str()? {
        "right-down" => RenderOrder::RightDown,
        r => return Err(anyhow!("Got an unsupported renderorder: {}", r)),
    };

    let orientation = match map_table.get::<_, LuaString>("orientation")?.to_str()? {
        "orthogonal" => Orientation::Orthogonal,
        "isometric" => Orientation::Isometric,
        o => return Err(anyhow!("Got an unsupported orientation: {}", o)),
    };

    Ok(MapMetaData {
        width: map_table.get("width")?,
        height: map_table.get("height")?,
        tilewidth: map_table.get("tilewidth")?,
        tileheight: map_table.get("tileheight")?,
        tsx_ver: map_table
            .get::<_, LuaString>("version")?
            .to_str()?
            .to_owned(),
        lua_ver: {
            match map_table.get::<_, LuaString>("luaversion") {
                Ok(s) => Some(s.to_str()?.to_owned()),
                Err(_) => None,
            }
        },
        tiled_ver: map_table
            .get::<_, LuaString>("tiledversion")?
            .to_str()?
            .to_owned(),
        nextlayerid: map_table.get::<_, LuaInteger>("nextlayerid")? as u32,
        nextobjectid: map_table.get::<_, LuaInteger>("nextobjectid")? as u32,
        properties: parse_properties(map_table)?,
        orientation,
        render_order,
    })
}

fn parse_chunk(
    t: &LuaTable,
    encoding: &Encoding,
    compression: &Option<Compression>,
    tile_buffer: &[u32],
) -> Result<(Chunk, i32, i32), Error> {
    let width: u32 = t.get("width")?;
    let height: u32 = t.get("height")?;

    assert_eq!(
        width, CHUNK_SIZE,
        "AHHHHH chunk sizes should always be 16! Got {} for width",
        width
    );
    assert_eq!(
        height, CHUNK_SIZE,
        "AHHHHH chunk sizes should always be 16! Got {} for height",
        height
    );

    Ok((
        Chunk(TileLayer::parse_tile_data(
            encoding,
            compression,
            t,
            tile_buffer,
        )?),
        t.get("x")?,
        t.get("y")?,
    ))
}

fn parse_tile_layer(t: &LuaTable, llid: u32, tile_buffer: &[u32]) -> Result<TileLayer, Error> {
    let layer_type = match t.get::<_, LuaString>("type")?.to_str()? {
        "tilelayer" => LayerType::Tile,
        s => return Err(anyhow!("Got an unsupported tilelayer type: {}", s)),
    };

    let encoding = match t.get::<_, LuaString>("encoding")?.to_str()? {
        "lua" => Encoding::Lua,
        "base64" => Encoding::Base64,
        e => return Err(anyhow!("Got an unsupported encoding type: {}", e)),
    };

    let compression = match t.get::<_, LuaString>("compression") {
        Ok(e) => match e.to_str()? {
            "gzip" => Some(Compression::GZip),
            "zlib" => Some(Compression::ZLib),
            "zstd" => return Err(anyhow!("Zstd compression is not supported!")),
            e => return Err(anyhow!("Got a corrupted compression format: {}", e)),
        },
        Err(_) => None,
    };
    let width = t.get("width")?;
    let height = t.get("height")?;

    let tile_data = if !t.contains_key("data")? {
        let mut chunks = HashMap::new();
        for chunk in t
            .get::<_, LuaTable>("chunks")?
            .sequence_values::<LuaTable>()
        {
            let (chunk, tile_x, tile_y) =
                parse_chunk(&chunk?, &encoding, &compression, tile_buffer)?;
            let chunk_x = tile_x / CHUNK_SIZE as i32;
            let chunk_y = tile_y / CHUNK_SIZE as i32;
            chunks.insert((chunk_x, chunk_y), chunk);
        }
        Chunks(chunks)
    } else {
        to_chunks(
            &TileLayer::parse_tile_data(&encoding, &compression, t, tile_buffer)?,
            width,
            height,
        )
    };

    Ok(TileLayer {
        id: TileLayerId {
            glid: t.get("id")?,
            llid,
        },
        name: t.get::<_, LuaString>("name")?.to_str()?.to_owned(),
        x: t.get("x")?,
        y: t.get("y")?,
        visible: t.get("visible")?,
        opacity: t.get("opacity")?,
        offset_x: t.get("offsetx")?,
        offset_y: t.get("offsety")?,
        properties: parse_properties(t)?,
        data: tile_data,
        layer_type,
        width,
        height,
    })
}

fn parse_draw_order(t: &LuaTable) -> Result<DrawOrder, Error> {
    match t.get::<_, LuaString>("draworder")?.to_str()? {
        "topdown" => Ok(DrawOrder::TopDown),
        "index" => Ok(DrawOrder::Index),
        s => Err(anyhow!("Unsupported draw order: {}", s)),
    }
}

fn parse_halign(t: &LuaTable) -> Result<Halign, Error> {
    match t.get::<_, LuaString>("halign") {
        Ok(s) => match s.to_str()? {
            "left" => Ok(Halign::Left),
            "center" => Ok(Halign::Center),
            "right" => Ok(Halign::Right),
            "justify" => Ok(Halign::Justify),
            s => Err(anyhow!("Unsupported halign value: {}", s)),
        },
        Err(_) => Ok(Halign::Left),
    }
}

fn parse_valign(t: &LuaTable) -> Result<Valign, Error> {
    match t.get::<_, LuaString>("valign") {
        Ok(s) => match s.to_str()? {
            "top" => Ok(Valign::Top),
            "center" => Ok(Valign::Center),
            "bottom" => Ok(Valign::Bottom),
            s => Err(anyhow!("Unsupported valign value: {}", s)),
        },
        Err(_) => Ok(Valign::Top),
    }
}

fn parse_text(t_table: &LuaTable) -> Result<Text, Error> {
    let fontfamily = match t_table.get::<_, LuaString>("fontfamily") {
        Ok(s) => s.to_str()?.to_owned(),
        Err(_) => "sans-serif".to_owned(),
    };

    Ok(Text {
        text: t_table.get::<_, LuaString>("text")?.to_str()?.to_owned(),
        pixelsize: t_table.get("pixelsize").unwrap_or(16),
        wrapping: t_table.get("wrapping").unwrap_or(false),
        color: Color::from_tiled_lua_table(t_table)?,
        bold: t_table.get("bold").unwrap_or(false),
        italic: t_table.get("italic").unwrap_or(false),
        underline: t_table.get("underline").unwrap_or(false),
        strikeout: t_table.get("strikeout").unwrap_or(false),
        kerning: t_table.get("kerning").unwrap_or(true),
        halign: parse_halign(t_table)?,
        valign: parse_valign(t_table)?,
        fontfamily,
    })
}

fn parse_object(
    obj_table: &LuaTable,
    from_obj_layer: bool,
    tileset_ids: Option<&[u32]>,
) -> Result<Object, Error> {
    let lua_shape_res = match obj_table.get::<_, LuaString>("shape")?.to_str()? {
        "text" => LuaShapeResolution::Text(parse_text(obj_table)?),
        s => LuaShapeResolution::ObjectShape(ObjectShape::from_string(s)?),
    };

    let (shape, text) = match lua_shape_res {
        LuaShapeResolution::ObjectShape(s) => (Some(s), None),
        LuaShapeResolution::Text(t) => (None, Some(t)),
    };

    let tile_id = obj_table.get("gid").ok().map(|gid| {
        TileId::from_gid(
            gid,
            tileset_ids.expect("B-BAKANA!!!! GOT A TILE OBJECT WITHIN A TILESET!"),
        )
    });

    Ok(Object {
        id: ObjectId::new(obj_table.get("id")?, from_obj_layer),
        name: obj_table.get::<_, LuaString>("name")?.to_str()?.to_owned(),
        obj_type: obj_table.get::<_, LuaString>("type")?.to_str()?.to_owned(),
        x: obj_table.get("x")?,
        y: obj_table.get("y")?,
        width: obj_table.get("width")?,
        height: obj_table.get("height")?,
        properties: parse_properties(obj_table)?,
        rotation: obj_table.get("rotation")?,
        visible: obj_table.get("visible")?,
        tile_id,
        shape,
        text,
    })
}

fn parse_obj_group_type(t: &LuaTable) -> Result<ObjGroupType, Error> {
    match t.get::<_, LuaString>("type")?.to_str()? {
        "objectgroup" => Ok(ObjGroupType::ObjectGroup),
        s => Err(anyhow!("Unsupported object group type: {}", s)),
    }
}

fn parse_object_group(
    objg_table: &LuaTable,
    llid: u32,
    from_obj_layer: bool,
    slab: &mut slab::Slab<Object>,
    tileset_ids: Option<&[u32]>,
) -> Result<(ObjectGroup, Vec<(ObjectId, ObjectRef)>), Error> {
    let mut obj_ids_and_refs = Vec::new();
    let mut object_name_map = HashMap::new();

    for object in objg_table.get::<_, LuaTable>("objects")?.sequence_values() {
        let object = parse_object(&object?, from_obj_layer, tileset_ids)?;

        let val = object_name_map
            .entry(object.name.clone())
            .or_insert_with(Vec::new);
        val.push(object.id);

        obj_ids_and_refs.push((object.id, ObjectRef(slab.insert(object))));
    }

    let color = match objg_table.get::<_, LuaString>("color") {
        Ok(s) => Color::from_tiled_hex(s.to_str()?)?,
        Err(_) => Color::from_rgb(0xA0, 0xA0, 0x0A4),
    };

    Ok((
        ObjectGroup {
            id: ObjectLayerId {
                glid: objg_table.get("id")?,
                llid,
            },
            name: objg_table.get("name")?,
            opacity: objg_table.get("opacity")?,
            visible: objg_table.get("visible")?,
            layer_index: objg_table.get("layer_index").ok(),
            properties: parse_properties(objg_table)?,
            draworder: parse_draw_order(objg_table)?,
            obj_group_type: parse_obj_group_type(objg_table)?,
            tintcolor: objg_table.get::<_, Color>("tintcolor").ok(),
            off_x: objg_table.get("offsetx").unwrap_or(0),
            off_y: objg_table.get("offsety").unwrap_or(0),
            object_refs: obj_ids_and_refs.iter().map(|i| i.1).collect(),
            color,
            object_name_map,
        },
        obj_ids_and_refs,
    ))
}

fn parse_animation(t: LuaTable, tileset: u32) -> Result<Animation, Error> {
    let mut animation_buffer = Vec::new();
    for animation in t.sequence_values() {
        let animation: LuaTable = animation?;
        animation_buffer.push((
            TileId(
                animation.get("tileid")?,
                TileMetaData::new(tileset, false, false, false),
            ),
            animation.get("duration")?,
        ));
    }
    Ok(Animation(animation_buffer))
}

fn parse_tile(
    tile_table: &LuaTable,
    tileset_num: u32,
    slab: &mut slab::Slab<Object>,
) -> Result<Tile, Error> {
    let objectgroup = match tile_table.get::<_, LuaTable>("objectGroup") {
        Ok(t) => Some(parse_object_group(&t, u32::MAX, false, slab, None)?.0),
        Err(_) => None,
    };

    Ok(Tile {
        // We have to add 1 here, because Tiled Data stores TileIds + 1, so for consistency,
        // we add 1 here
        id: TileId(
            tile_table.get::<_, LuaInteger>("id")? as u32 + 1,
            TileMetaData::new(tileset_num, false, false, false),
        ),
        tile_type: tile_table.get("type").ok(),
        probability: tile_table.get("probability").unwrap_or(0.0),
        animation: match tile_table.get::<_, LuaTable>("animation") {
            Ok(t) => Some(parse_animation(t, tileset_num)?),
            Err(_) => None,
        },
        properties: match tile_table.get::<_, LuaTable>("properties") {
            Ok(_) => parse_properties(tile_table)?,
            Err(_) => Properties(HashMap::new()),
        },
        objectgroup,
    })
}

fn parse_tileset(
    ts: &LuaTable,
    path_prefix: Option<&str>,
    tileset_number: u32,
    slab: &mut slab::Slab<Object>,
) -> Result<Tileset, Error> {
    let mut tiles = HashMap::new();
    for tile_table in ts.get::<_, LuaTable>("tiles")?.sequence_values() {
        let tile = parse_tile(&tile_table?, tileset_number, slab)?;
        tiles.insert(tile.id, tile);
    }

    Ok(Tileset {
        name: ts.get::<_, LuaString>("name")?.to_str()?.to_owned(),
        first_gid: ts.get("firstgid")?,
        tile_width: ts.get("tilewidth")?,
        tile_height: ts.get("tileheight")?,
        spacing: ts.get("spacing")?,
        margin: ts.get("margin")?,
        columns: ts.get("columns")?,
        images: vec![Image::new(ts, path_prefix)?],
        tilecount: ts.get("tilecount")?,
        properties: parse_properties(ts)?,
        tiles,
    })
}

pub fn parse_map(
    map_path: &str,
    fs: &mut hv::fs::Filesystem,
    lua: &Lua,
    path_prefix: Option<&str>,
) -> Result<Map, Error> {
    let mut tiled_lua_map = fs.open(Path::new(map_path))?;
    let mut tiled_buffer: Vec<u8> = Vec::new();
    tiled_lua_map.read_to_end(&mut tiled_buffer)?;
    let lua_chunk = lua.load(&tiled_buffer);
    let tiled_lua_table = lua_chunk.eval::<LuaTable>()?;
    let meta_data = parse_map_meta_data(&tiled_lua_table)?;

    let mut tilesets = Vec::new();
    // We initialize the tile_buffer with 1 0'd out TileId to account for the fact
    // that layer indexing starts at 1 instead of 0
    let mut tile_buffer = vec![0];
    let mut obj_slab = slab::Slab::new();

    for (tileset, i) in tiled_lua_table
        .get::<_, LuaTable>("tilesets")?
        .sequence_values::<LuaTable>()
        .zip(0..)
    {
        let tileset = parse_tileset(&tileset?, path_prefix, i, &mut obj_slab)?;
        tile_buffer.reserve(tileset.tilecount as usize);
        for _ in tileset.first_gid..tileset.tilecount {
            tile_buffer.push(i);
        }
        tilesets.push(tileset);
    }

    let mut tile_layers = Vec::new();
    let mut object_layers = Vec::new();

    let mut tile_layer_map = HashMap::new();
    let mut object_layer_map = HashMap::new();

    let mut obj_id_to_ref_map = HashMap::new();

    let mut tile_llid = 0;
    let mut obj_llid = 0;

    for layer in tiled_lua_table
        .get::<_, LuaTable>("layers")?
        .sequence_values::<LuaTable>()
    {
        let layer = layer?;
        let layer_type = parse_layer_type(&layer)?;
        match layer_type {
            LayerType::Tile => {
                let tile_layer = parse_tile_layer(&layer, tile_llid, &tile_buffer)?;
                tile_layer_map.insert(tile_layer.name.clone(), tile_layer.id);
                tile_layers.push(tile_layer);
                tile_llid += 1;
            }
            LayerType::Object => {
                let (obj_group, obj_ids_and_refs) =
                    parse_object_group(&layer, obj_llid, true, &mut obj_slab, Some(&tile_buffer))?;
                for (obj_id, obj_ref) in obj_ids_and_refs.iter() {
                    obj_id_to_ref_map.insert(*obj_id, *obj_ref);
                }
                object_layer_map.insert(obj_group.name.clone(), obj_group.id);
                object_layers.push(obj_group);
                obj_llid += 1;
            }
        }
    }

    // drop(tiled_lua_table); TODO: do we need this line?

    Ok(Map::new(
        meta_data,
        tile_layers,
        object_layers,
        Tilesets(tilesets),
        tile_layer_map,
        object_layer_map,
        obj_slab,
        obj_id_to_ref_map,
    ))
}
