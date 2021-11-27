use hv::prelude::*;
use shrev::EventChannel;

use crate::lattice::{
    chunk_map::{Chunk, ChunkLayer, ChunkMap, DividedCoords},
    event::{
        ChunkEvent, ChunkEventKind, LatticeEvent, LayerEvent, LayerEventKind, SlotEvent,
        SlotEventKind,
    },
    ChunkCoords, SubCoords,
};

pub struct TrackedChunk<'a, T: Copy + Send + Sync + 'static> {
    layer: i32,
    chunk: ChunkCoords,
    chunk_mut: &'a mut Chunk<T>,
    channel: &'a mut EventChannel<LatticeEvent<T>>,
}

impl<'a, T: Copy + Send + Sync + 'static> TrackedChunk<'a, T> {
    pub fn insert(&mut self, sub: SubCoords, value: T) -> Option<T> {
        let prev = self.chunk_mut.insert(sub, value);
        self.channel.single_write(LatticeEvent::Slot(SlotEvent {
            layer: self.layer,
            chunk: self.chunk,
            sub,
            kind: SlotEventKind::Insert { new: value, prev },
        }));
        prev
    }

    pub fn get(&self, sub: SubCoords) -> Option<T> {
        self.chunk_mut.get(sub).copied()
    }

    pub fn remove(&mut self, sub: SubCoords) -> Option<T> {
        let removed = self.chunk_mut.remove(sub);

        if let Some(prev) = removed {
            self.channel.single_write(LatticeEvent::Slot(SlotEvent {
                layer: self.layer,
                chunk: self.chunk,
                sub,
                kind: SlotEventKind::Remove { prev },
            }));
        }

        removed
    }

    pub fn as_chunk(&self) -> &Chunk<T> {
        self.chunk_mut
    }

    pub fn as_chunk_mut(&mut self) -> &mut Chunk<T> {
        self.chunk_mut
    }
}

pub struct TrackedLayer<'a, T: Copy + Send + Sync + 'static> {
    layer: i32,
    layer_mut: &'a mut ChunkLayer<T>,
    channel: &'a mut EventChannel<LatticeEvent<T>>,
}

impl<'a, T: Copy + Send + Sync + 'static> TrackedLayer<'a, T> {
    pub fn get_chunk(&self, coords: ChunkCoords) -> Option<&Chunk<T>> {
        self.layer_mut.get_chunk(coords)
    }

    pub fn get_chunk_mut(&mut self, coords: ChunkCoords) -> Option<TrackedChunk<T>> {
        let chunk_mut = self.layer_mut.get_chunk_mut(coords)?;
        Some(TrackedChunk {
            layer: self.layer,
            chunk: coords,
            chunk_mut,
            channel: self.channel,
        })
    }

    pub fn get_or_insert_chunk(&mut self, coords: ChunkCoords) -> TrackedChunk<T> {
        let chunk_mut = self.layer_mut.get_or_insert_chunk(coords);
        TrackedChunk {
            layer: self.layer,
            chunk: coords,
            chunk_mut,
            channel: self.channel,
        }
    }

    pub fn insert(&mut self, coords: Vector2<i32>, value: T) -> Option<T> {
        let divided = DividedCoords::from_world_coords(coords);
        let mut chunk = self.get_or_insert_chunk(divided.chunk_coords);
        chunk.insert(divided.sub_coords, value)
    }

    pub fn get(&self, coords: Vector2<i32>) -> Option<T> {
        self.layer_mut.get(coords).copied()
    }

    pub fn remove(&mut self, coords: Vector2<i32>) -> Option<T> {
        let divided = DividedCoords::from_world_coords(coords);
        let mut chunk = self.get_chunk_mut(divided.chunk_coords)?;
        chunk.remove(divided.sub_coords)
    }

    pub fn insert_chunk(&mut self, coords: ChunkCoords, chunk: Chunk<T>) -> Option<Chunk<T>> {
        self.channel.single_write(LatticeEvent::Chunk(ChunkEvent {
            layer: self.layer,
            chunk: coords,
            kind: ChunkEventKind::Insert,
        }));

        self.layer_mut.insert_chunk(coords, chunk)
    }

    pub fn remove_chunk(&mut self, coords: ChunkCoords) -> Option<Chunk<T>> {
        let removed = self.layer_mut.remove_chunk(coords);

        // Only write the event if a chunk is actually removed.
        if removed.is_some() {
            self.channel.single_write(LatticeEvent::Chunk(ChunkEvent {
                layer: self.layer,
                chunk: coords,
                kind: ChunkEventKind::Remove,
            }));
        }

        removed
    }

    pub fn as_layer(&self) -> &ChunkLayer<T> {
        self.layer_mut
    }

    pub fn as_layer_mut(&mut self) -> &mut ChunkLayer<T> {
        self.layer_mut
    }
}

#[derive(Debug)]
pub struct TrackedMap<T: Copy + Send + Sync + 'static> {
    map: ChunkMap<T>,
    channel: EventChannel<LatticeEvent<T>>,
}

impl<T: Copy + Send + Sync + 'static> Default for TrackedMap<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy + Send + Sync + 'static> TrackedMap<T> {
    pub fn new() -> Self {
        Self {
            map: ChunkMap::new(),
            channel: EventChannel::new(),
        }
    }

    pub fn get_layer(&self, index: i32) -> Option<&ChunkLayer<T>> {
        self.map.get_layer(index)
    }

    pub fn get_layer_mut(&mut self, index: i32) -> Option<TrackedLayer<T>> {
        let layer_mut = self.map.get_layer_mut(index)?;
        Some(TrackedLayer {
            layer: index,
            layer_mut,
            channel: &mut self.channel,
        })
    }

    pub fn get_or_insert_layer(&mut self, index: i32) -> TrackedLayer<T> {
        let layer_mut = self.map.get_or_insert_layer(index);
        TrackedLayer {
            layer: index,
            layer_mut,
            channel: &mut self.channel,
        }
    }

    pub fn insert(&mut self, coords: Vector3<i32>, value: T) -> Option<T> {
        self.get_or_insert_layer(coords.z)
            .insert(coords.xy(), value)
    }

    pub fn insert_chunk(
        &mut self,
        layer: i32,
        chunk_coords: ChunkCoords,
        chunk: Chunk<T>,
    ) -> Option<Chunk<T>> {
        self.get_layer_mut(layer)?.insert_chunk(chunk_coords, chunk)
    }

    pub fn remove_chunk(&mut self, layer: i32, chunk_coords: ChunkCoords) -> Option<Chunk<T>> {
        self.get_layer_mut(layer)?.remove_chunk(chunk_coords)
    }

    pub fn insert_layer(&mut self, index: i32, layer: ChunkLayer<T>) -> Option<ChunkLayer<T>> {
        self.channel.single_write(LatticeEvent::Layer(LayerEvent {
            layer: index,
            kind: LayerEventKind::Insert,
        }));

        self.map.insert_layer(index, layer)
    }

    pub fn remove_layer(&mut self, index: i32) -> Option<ChunkLayer<T>> {
        let removed = self.map.remove_layer(index);

        // Only record the event if a layer is *actually* removed.
        if removed.is_some() {
            self.channel.single_write(LatticeEvent::Layer(LayerEvent {
                layer: index,
                kind: LayerEventKind::Remove,
            }));
        }

        removed
    }

    pub fn get(&self, coords: Vector3<i32>) -> Option<&T> {
        self.get_layer(coords.z)
            .and_then(|layer| layer.get(coords.xy()))
    }

    pub fn as_chunk_map(&self) -> &ChunkMap<T> {
        &self.map
    }

    pub fn as_chunk_map_mut(&mut self) -> &mut ChunkMap<T> {
        &mut self.map
    }

    pub fn events(&self) -> &EventChannel<LatticeEvent<T>> {
        &self.channel
    }

    pub fn events_mut(&mut self) -> &mut EventChannel<LatticeEvent<T>> {
        &mut self.channel
    }
}
