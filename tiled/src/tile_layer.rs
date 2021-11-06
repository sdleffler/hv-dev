use crate::*;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct TileLayerId {
    // global layer id and local layer id
    // global layer id is set by tiled, local layer id is generated sequentially in the order
    // that the layers are parsed
    pub glid: u32,
    pub llid: u32,
}

#[derive(Debug, Clone)]
pub struct Chunk(pub Vec<TileId>);

impl Chunk {
    fn new() -> Self {
        Chunk(vec![EMPTY_TILE; (CHUNK_SIZE * CHUNK_SIZE) as usize])
    }
}

fn to_chunk_indices_and_subindices(x: i32, y: i32) -> (i32, i32, u32, u32) {
    let y = -y;
    let (chunk_x, tile_x) = (
        x.div_euclid(CHUNK_SIZE as i32),
        x.rem_euclid(CHUNK_SIZE as i32) as u32,
    );
    let (chunk_y, tile_y) = (
        y.div_euclid(CHUNK_SIZE as i32),
        y.rem_euclid(CHUNK_SIZE as i32) as u32,
    );
    (chunk_x, chunk_y, tile_x, tile_y)
}

#[derive(Debug, Default, Clone)]
pub struct Chunks(pub HashMap<(i32, i32), Chunk>);

impl Chunks {
    pub fn new() -> Self {
        Chunks(HashMap::default())
    }

    pub fn set_tile(&mut self, x: i32, y: i32, tile: TileId) -> Option<TileId> {
        let (chunk_x, chunk_y, tile_x, tile_y) = to_chunk_indices_and_subindices(x, y);
        let chunk = self.0.entry((chunk_x, chunk_y)).or_insert_with(Chunk::new);
        let index = (tile_y * CHUNK_SIZE + tile_x) as usize;
        let tile_id = chunk.0[index];
        chunk.0[index] = tile;
        if tile_id != EMPTY_TILE {
            Some(tile_id)
        } else {
            None
        }
    }

    pub fn remove_tile(&mut self, x: i32, y: i32) -> Option<TileId> {
        let (chunk_x, chunk_y, tile_x, tile_y) = to_chunk_indices_and_subindices(x, y);
        if let Some(chunk) = self.0.get_mut(&(chunk_x, chunk_y)) {
            let index = (tile_y * CHUNK_SIZE + tile_x) as usize;
            let tile_id = chunk.0[index];
            chunk.0[index] = EMPTY_TILE;
            if tile_id != EMPTY_TILE {
                Some(tile_id)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn get_tile(&self, x: i32, y: i32) -> Option<TileId> {
        let (chunk_x, chunk_y, tile_x, tile_y) = to_chunk_indices_and_subindices(x, y);
        self.0.get(&(chunk_x, chunk_y)).and_then(|chunk| {
            match chunk.0[((CHUNK_SIZE * tile_y) + tile_x) as usize] {
                EMPTY_TILE => None,
                t => Some(t),
            }
        })
    }
}

pub fn to_chunks(data: &[TileId], width: u32, height: u32) -> Chunks {
    let mut chunks = Chunks::default();
    for y in 0..height {
        for x in 0..width {
            chunks.set_tile(x as i32, y as i32, data[(y * width + x) as usize]);
        }
    }
    chunks
}

#[derive(Debug, Clone)]
pub enum Encoding {
    Lua,
    Base64,
}

#[derive(Debug, Clone)]
pub enum Compression {
    ZLib,
    GZip,
}

#[derive(Debug, Clone)]
pub struct TileLayer {
    pub layer_type: LayerType,
    pub id: TileLayerId,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub visible: bool,
    pub opacity: f64,
    pub offset_x: i32,
    pub offset_y: i32,
    pub properties: Properties,
    pub data: Chunks,
}

impl TileLayer {
    pub fn parse_tile_data(
        encoding: &Encoding,
        compression: &Option<Compression>,
        t: &LuaTable,
        tile_buffer: &[u32],
    ) -> Result<Vec<TileId>, Error> {
        // TODO: Vector capacity can be pre-calculated here, optimize this
        let mut tile_data = Vec::new();

        match encoding {
            Encoding::Lua => {
                for tile in t
                    .get::<_, LuaTable>("data")?
                    .sequence_values::<LuaInteger>()
                {
                    tile_data.push(tile? as u32);
                }
            }

            Encoding::Base64 => {
                let str_data = t.get::<_, LuaString>("data")?.to_str()?.to_owned();

                let decoded_bytes = base64::decode_config(str_data, base64::STANDARD)?;

                let level_bytes = match compression {
                    Some(c) => match c {
                        Compression::GZip => {
                            let mut d = flate2::read::GzDecoder::new(decoded_bytes.as_slice());
                            let mut s = Vec::new();
                            d.read_to_end(&mut s).unwrap();
                            s
                        }
                        Compression::ZLib => {
                            let mut d = flate2::read::ZlibDecoder::new(decoded_bytes.as_slice());
                            let mut s = Vec::new();
                            d.read_to_end(&mut s).unwrap();
                            s
                        }
                    },
                    None => decoded_bytes,
                };

                for i in (0..level_bytes.len()).step_by(4) {
                    let val = level_bytes[i] as u32
                        | (level_bytes[i + 1] as u32) << 8
                        | (level_bytes[i + 2] as u32) << 16
                        | (level_bytes[i + 3] as u32) << 24;
                    tile_data.push(val);
                }
            }
        }

        Ok(tile_data
            .into_iter()
            .map(|tile| TileId::from_gid(tile, tile_buffer))
            .collect())
    }
}
