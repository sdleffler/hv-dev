use crate::*;
use hv::prelude::*;

impl Tileset {
    pub fn json_parse_tileset(
        v: &Value,
        first_gid: u32,
        path_prefix: Option<&str>,
        tileset_number: u8,
        slab: &mut slab::Slab<Object>,
        filename: String,
    ) -> Result<Self> {
        let json_obj = v
            .as_object()
            .ok_or(anyhow!("Tileset file did not contain a json dictionary"))?;

        let tile_array =
            json_obj
                .get("tiles")
                .map_or::<Result<&[Value]>, _>(Ok(&[][..]), |t_arr| {
                    Ok(t_arr
                        .as_array()
                        .ok_or_else(|| anyhow!("Tiles are not an array"))?
                        .as_slice())
                })?;

        let mut tiles = HashMap::new();

        for tile_obj in tile_array.iter() {
            let tile = Tile::json_parse_tile(tile_obj, tileset_number, slab)?;
            tiles.insert(tile.id, tile);
        }

        Ok(Tileset {
            columns: json_obj
                .get("columns")
                .ok_or_else(|| anyhow!("Should've gotten columns"))?
                .as_u64()
                .ok_or_else(|| anyhow!("Columns value wasn't a u64"))?
                .try_into()
                .expect("Bruh how many columns does your tileset have"),

            images: vec![Image::from_json(json_obj, path_prefix)?],
            tilecount: json_obj
                .get("tilecount")
                .ok_or_else(|| anyhow!("Should've gotten tilecount"))?
                .as_u64()
                .ok_or_else(|| anyhow!("Tilecount value wasn't a u64"))?
                .try_into()
                .expect("Bruh how many tiles does your tileset have"),
            tile_width: json_obj
                .get("tilewidth")
                .ok_or_else(|| anyhow!("Should've gotten tilewidth"))?
                .as_u64()
                .ok_or_else(|| anyhow!("Tilewidth value wasn't a u64"))?
                .try_into()
                .expect("Tiles are too thicc"),
            tile_height: json_obj
                .get("tileheight")
                .ok_or_else(|| anyhow!("Should've gotten tileheight"))?
                .as_u64()
                .ok_or_else(|| anyhow!("Tileheight value wasn't a u64"))?
                .try_into()
                .expect("Tiles are too tall owo"),
            spacing: json_obj
                .get("spacing")
                .ok_or_else(|| anyhow!("Should've gotten spacing"))?
                .as_u64()
                .ok_or_else(|| anyhow!("Spacing value wasn't a u64"))?
                .try_into()
                .expect(
                    "God help you if you actually have 2,147,483,647 pixels in between each tile",
                ),
            name: json_obj
                .get("name")
                .ok_or_else(|| anyhow!("Should've gotten a name"))?
                .as_str()
                .ok_or_else(|| anyhow!("Name wasn't a valid string"))?
                .to_owned(),
            margin: json_obj
                .get("margin")
                .ok_or_else(|| anyhow!("Should've gotten a margin"))?
                .as_u64()
                .ok_or_else(|| anyhow!("Margin value wasn't a u64"))?
                .try_into()
                .expect(
                    "God help you if you actually have 2,147,483,647 pixels AROUND your tileset",
                ),
            properties: Properties::json_parse_properties(v)?,
            filename: Some(filename),
            tiles,
            first_gid,
        })
    }
}

impl Animation {
    fn json_parse_animation(v: &[Value], tileset: u8) -> Result<Self> {
        let mut animation_frames = Vec::with_capacity(v.len());
        for entry in v.iter() {
            animation_frames.push((
                TileId(
                    entry
                        .get("tileid")
                        .ok_or_else(|| anyhow!("Couldn't find a tileid in the animation"))?
                        .as_u64()
                        .ok_or_else(|| anyhow!("Tileid should be a u64"))?
                        .try_into()
                        .expect("Tile ids should fit into u32s probably"),
                    TileMetaData::new(tileset, false, false, false),
                ),
                entry
                    .get("duration")
                    .ok_or_else(|| anyhow!("Couldn't find a duration in the animation"))?
                    .as_u64()
                    .ok_or_else(|| anyhow!("Duration should be a u64"))?
                    .try_into()
                    .expect("Duration should probably fit in a u32"),
            ));
        }
        Ok(Animation(animation_frames))
    }
}

impl Properties {
    fn json_parse_properties(v: &Value) -> Result<Self> {
        let mut properties = HashMap::new();

        if let Some(p) = v.get("properties") {
            let properties_arr = p
                .as_array()
                .ok_or_else(|| anyhow!("Couldn't turn properties into an array"))?;
            if properties_arr.len() > 1 {
                return Err(anyhow!(
                    "Properties array was greater than 1, not sure if this is expected"
                ));
            }
            for (k, v) in properties_arr[0]
                .as_object()
                .ok_or_else(|| {
                    anyhow!("Properties first element couldn't be turned into an object")
                })?
                .iter()
            {
                properties.insert(k.clone(), Property::from_json_entry(v)?);
            }
        }

        Ok(Properties(properties))
    }
}

impl Tile {
    fn json_parse_tile(v: &Value, tileset_num: u8, slab: &mut slab::Slab<Object>) -> Result<Self> {
        let objectgroup = match v.get("objectGroup") {
            Some(v) => {
                Some(ObjectGroup::json_parse_object_group(v, u32::MAX, false, slab, None)?.0)
            }
            None => None,
        };
        let tile_id: u32 = v
            .get("id")
            .ok_or_else(|| anyhow!("Tile entry had no tile id"))?
            .as_u64()
            .ok_or_else(|| anyhow!("Could not turn tile id into u64"))?
            .try_into()
            .expect("Tile id greater than max u32");

        Ok(Tile {
            id: TileId(
                tile_id + 1,
                TileMetaData::new(tileset_num, false, false, false),
            ),
            tile_type: v
                .get("type")
                .map(|s| {
                    s.as_str()
                        .map(ToOwned::to_owned)
                        .ok_or_else(|| anyhow!("Tile type wasn't a string"))
                })
                .transpose()?,
            probability: v
                .get("probability")
                .map(Value::as_f64)
                .unwrap_or(Some(0.0))
                .ok_or_else(|| anyhow!("Probability wasn't a float"))?
                as f32,
            properties: Properties::json_parse_properties(v)?,
            animation: v
                .get("animation")
                .map(|a| {
                    Animation::json_parse_animation(
                        a.as_array()
                            .ok_or_else(|| anyhow!("Animation values weren't an array"))?,
                        tileset_num,
                    )
                })
                .transpose()?,
            objectgroup,
        })
    }
}

impl ObjectGroup {
    fn json_parse_object_group(
        objg_obj: &Value,
        llid: u32,
        from_obj_layer: bool,
        slab: &mut slab::Slab<Object>,
        tileset_ids: Option<&[u8]>,
    ) -> Result<(ObjectGroup, Vec<(ObjectId, ObjectRef)>), Error> {
        let mut obj_ids_and_refs = Vec::new();
        let mut object_name_map = HashMap::new();

        for object in objg_obj
            .get("objects")
            .ok_or_else(|| anyhow!("Didn't find objects in the objectgroup"))?
            .as_array()
            .ok_or_else(|| anyhow!("Couldn't retrieve objects as an array"))?
            .iter()
        {
            let object = Object::json_parse_object(object, from_obj_layer, tileset_ids)?;

            let val = object_name_map
                .entry(object.name.clone())
                .or_insert_with(Vec::new);
            val.push(object.id);

            obj_ids_and_refs.push((object.id, ObjectRef(slab.insert(object))));
        }

        Ok((
            ObjectGroup {
                name: objg_obj
                    .get("name")
                    .ok_or_else(|| anyhow!("Object group did not have a name"))?
                    .as_str()
                    .ok_or_else(|| anyhow!("Name couldn't be converted to a string"))?
                    .to_owned(),
                opacity: objg_obj
                    .get("opacity")
                    .ok_or_else(|| anyhow!("Object group did not have an opacity"))?
                    .as_f64()
                    .ok_or_else(|| anyhow!("Opacity couldn't be converted to a f64"))?
                    as f32,
                visible: objg_obj
                    .get("visible")
                    .ok_or_else(|| anyhow!("Object group did not have a visibility"))?
                    .as_bool()
                    .ok_or_else(|| anyhow!("Visibility couldn't be converted to a bool"))?,
                obj_group_type: ObjGroupType::json_parse_obj_group_type(objg_obj)?,
                properties: Properties::json_parse_properties(objg_obj)?,
                draworder: DrawOrder::json_parse_draw_order(objg_obj)?,
                id: ObjectLayerId {
                    glid: objg_obj
                        .get("id")
                        .ok_or_else(|| anyhow!("Object group did not have an id"))?
                        .as_u64()
                        .ok_or_else(|| anyhow!("Id couldn't be converted to a u64"))?
                        .try_into()
                        .expect("Too many objects"),
                    llid,
                },
                layer_index: objg_obj
                    .get("layer_index")
                    .map(|l_i| {
                        l_i.as_u64()
                            .ok_or_else(|| anyhow!("Layer index couldn't be turned into a u64"))
                            .map(|n| n.try_into().expect("Layer indexes too large"))
                    })
                    .transpose()?,
                off_x: objg_obj
                    .get("x")
                    .ok_or_else(|| anyhow!("Didn't find x offset in object group"))?
                    .as_u64()
                    .ok_or_else(|| anyhow!("Couldn't turn x offset to u64"))?
                    .try_into()
                    .expect("X offset too large"),
                off_y: objg_obj
                    .get("y")
                    .ok_or_else(|| anyhow!("Didn't find y offset in object group"))?
                    .as_u64()
                    .ok_or_else(|| anyhow!("Couldn't turn y offset to u64"))?
                    .try_into()
                    .expect("Y offset too large"),
                color: objg_obj.get("color").map_or(
                    Ok(Color::from_rgb(0xA0, 0xA0, 0xA4)),
                    |c| {
                        c.as_str()
                            .ok_or_else(|| anyhow!("Color wasn't a string"))
                            .and_then(Color::from_tiled_hex)
                    },
                )?,
                tintcolor: objg_obj
                    .get("tintcolor")
                    .map(|s| {
                        s.as_str()
                            .ok_or_else(|| anyhow!("Tintcolor value wasn't a string"))
                            .and_then(Color::from_tiled_hex)
                    })
                    .transpose()?,
                object_refs: obj_ids_and_refs.iter().map(|i| i.1).collect(),
                object_name_map,
            },
            obj_ids_and_refs,
        ))
    }
}

impl ObjGroupType {
    fn json_parse_obj_group_type(v: &Value) -> Result<Self> {
        match v
            .get("type")
            .ok_or_else(|| anyhow!("Object group did not contain key type"))?
            .as_str()
            .ok_or_else(|| anyhow!("Object group type couldn't be turned into a string"))?
        {
            "objectgroup" => Ok(ObjGroupType::ObjectGroup),
            s => Err(anyhow!("Unsupported object group type: {}", s)),
        }
    }
}

impl DrawOrder {
    fn json_parse_draw_order(v: &Value) -> Result<Self> {
        match v
            .get("draworder")
            .ok_or_else(|| anyhow!("Object group did not contain draworder"))?
            .as_str()
            .ok_or_else(|| anyhow!("Draworder couldn't be turned into a string"))?
        {
            "index" => Ok(DrawOrder::Index),
            s => Err(anyhow!("Unsupported draworder: {}", s)),
        }
    }
}

impl Object {
    fn json_parse_object(
        object: &Value,
        from_obj_layer: bool,
        // TODO: tileset_ids will be used for parsing object groups from object layers
        _tileset_ids: Option<&[u8]>,
    ) -> Result<Self> {
        Ok(Object {
            name: object
                .get("name")
                .ok_or_else(|| anyhow!("Object did not have a name"))?
                .as_str()
                .ok_or_else(|| anyhow!("Name couldn't be converted to a string"))?
                .to_owned(),
            visible: object
                .get("visible")
                .ok_or_else(|| anyhow!("Object did not have a visibility"))?
                .as_bool()
                .ok_or_else(|| anyhow!("Visibility couldn't be converted to a bool"))?,
            obj_type: object
                .get("type")
                .ok_or_else(|| anyhow!("Object did not have a type"))?
                .as_str()
                .ok_or_else(|| anyhow!("Visibility couldn't be converted to a bool"))?
                .to_owned(),
            height: object
                .get("height")
                .ok_or_else(|| anyhow!("Object did not have a height"))?
                .as_f64()
                .ok_or_else(|| anyhow!("Height couldn't be converted to a f64"))?
                as f32,
            width: object
                .get("width")
                .ok_or_else(|| anyhow!("Object did not have a width"))?
                .as_f64()
                .ok_or_else(|| anyhow!("Width couldn't be converted to a f64"))?
                as f32,
            rotation: object
                .get("rotation")
                .ok_or_else(|| anyhow!("Object did not have a rotation"))?
                .as_f64()
                .ok_or_else(|| anyhow!("Rotation couldn't be converted to a f64"))?
                as f32,
            x: object
                .get("x")
                .ok_or_else(|| anyhow!("Object did not have an x pos"))?
                .as_f64()
                .ok_or_else(|| anyhow!("X pos couldn't be converted to a f64"))?
                as f32,
            y: object
                .get("y")
                .ok_or_else(|| anyhow!("Object did not have an y pos"))?
                .as_f64()
                .ok_or_else(|| anyhow!("Y pos couldn't be converted to a f64"))?
                as f32,
            properties: Properties::json_parse_properties(object)?,
            text: None, // TODO: I don't know if text can be attached to object groups as part of tilesets
            tile_id: None, // I don't think you can have tile IDs in a tileset
            id: ObjectId::new(
                object
                    .get("id")
                    .ok_or_else(|| anyhow!("Object did not have an ID"))?
                    .as_u64()
                    .ok_or_else(|| anyhow!("ID couldn't be represented as u64"))?
                    .try_into()
                    .expect("ID greater than u32 MAX"),
                from_obj_layer,
            ),
            shape: Some(ObjectShape::from_json(object)?),
        })
    }
}
