use std::{
    collections::{BTreeMap, HashMap},
    mem::MaybeUninit,
    ops::{Deref, Index, IndexMut, RangeBounds},
    ptr::NonNull,
};

use hv::prelude::*;

pub const CHUNK_SIDE_LENGTH: usize = 16;
pub const CHUNK_AREA: usize = CHUNK_SIDE_LENGTH * CHUNK_SIDE_LENGTH;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Hash)]
pub struct ChunkCoords(Vector2<i32>);

impl Deref for ChunkCoords {
    type Target = Vector2<i32>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ChunkCoords {
    pub fn new(x: i32, y: i32) -> Self {
        Self(Vector2::new(x, y))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SubCoords(Vector2<u32>);

impl Deref for SubCoords {
    type Target = Vector2<u32>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl SubCoords {
    pub fn new(v: Vector2<u32>) -> Self {
        assert!(
            v.x < CHUNK_SIDE_LENGTH as u32,
            "x coordinate out of bounds!"
        );
        assert!(
            v.y < CHUNK_SIDE_LENGTH as u32,
            "y coordinate out of bounds!"
        );
        Self(v)
    }

    pub fn to_linear(self) -> usize {
        let v = self.cast::<usize>();
        v.y * CHUNK_SIDE_LENGTH + v.x
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DividedCoords {
    pub chunk_coords: ChunkCoords,
    pub sub_coords: SubCoords,
}

impl DividedCoords {
    pub fn from_world_coords(coords: Vector2<i32>) -> Self {
        let chunk_coords = coords.map(|t| t.div_euclid(CHUNK_SIDE_LENGTH as i32));
        let sub_coords = coords
            .map(|t| t.rem_euclid(CHUNK_SIDE_LENGTH as i32))
            .map(|t| t as u32);
        Self {
            chunk_coords: ChunkCoords(chunk_coords),
            sub_coords: SubCoords(sub_coords),
        }
    }

    pub fn to_world_coords(self) -> Vector2<i32> {
        *self.chunk_coords * (CHUNK_SIDE_LENGTH as i32) + self.sub_coords.cast::<i32>()
    }
}

#[derive(Debug, Clone)]
pub struct Chunk<T> {
    data: Box<[T; CHUNK_AREA]>,
}

impl<T> Index<SubCoords> for Chunk<T> {
    type Output = T;

    fn index(&self, index: SubCoords) -> &Self::Output {
        &self.data[index.to_linear()]
    }
}

impl<T> IndexMut<SubCoords> for Chunk<T> {
    fn index_mut(&mut self, index: SubCoords) -> &mut Self::Output {
        &mut self.data[index.to_linear()]
    }
}

impl<T: Default> Default for Chunk<T> {
    fn default() -> Self {
        let mut array = MaybeUninit::uninit_array();
        array
            .iter_mut()
            .for_each(|t| drop(t.write(Default::default())));
        Chunk {
            data: Box::new(unsafe { MaybeUninit::array_assume_init(array) }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChunkLayer<T> {
    chunks: HashMap<ChunkCoords, Chunk<T>>,
}

impl<T> Default for ChunkLayer<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> ChunkLayer<T> {
    pub fn new() -> Self {
        Self {
            chunks: HashMap::new(),
        }
    }

    pub fn get_chunk(&self, coords: ChunkCoords) -> Option<&Chunk<T>> {
        self.chunks.get(&coords)
    }

    pub fn get_chunk_mut(&mut self, coords: ChunkCoords) -> Option<&mut Chunk<T>> {
        self.chunks.get_mut(&coords)
    }

    pub fn insert_chunk(&mut self, coords: ChunkCoords, chunk: Chunk<T>) -> Option<Chunk<T>> {
        self.chunks.insert(coords, chunk)
    }

    pub fn remove_chunk(&mut self, coords: ChunkCoords) -> Option<Chunk<T>> {
        self.chunks.remove(&coords)
    }

    pub fn get_slot(&self, coords: Vector2<i32>) -> Option<&T> {
        let divided = DividedCoords::from_world_coords(coords);
        self.get_chunk(divided.chunk_coords)
            .map(|chunk| &chunk[divided.sub_coords])
    }

    pub fn get_slot_mut(&mut self, coords: Vector2<i32>) -> Option<&mut T> {
        let divided = DividedCoords::from_world_coords(coords);
        self.get_chunk_mut(divided.chunk_coords)
            .map(|chunk| &mut chunk[divided.sub_coords])
    }

    pub fn get_n_slots_mut<const N: usize>(
        &mut self,
        coords: [Vector2<i32>; N],
    ) -> [Option<&mut T>; N] {
        // Fast if N is small.
        for i in 0..N {
            for j in i..N {
                assert_ne!(
                    coords[i], coords[j],
                    "cannot mutably borrow the same slot twice!"
                );
            }
        }

        let mut ptrs = [None; N];
        for i in 0..N {
            let t = self.get_slot_mut(coords[i]);
            ptrs[i] = t.map(|mut_ref| NonNull::new(mut_ref as *mut T).unwrap());
        }

        // Safe: these all point to different elements.
        ptrs.map(|t| t.map(|mut nn| unsafe { nn.as_mut() }))
    }

    pub fn get_all_n_slots_mut<const N: usize>(
        &mut self,
        coords: [Vector2<i32>; N],
    ) -> Option<[&mut T; N]> {
        let slots = self.get_n_slots_mut(coords);

        if slots.iter().all(Option::is_some) {
            Some(slots.map(Option::unwrap))
        } else {
            None
        }
    }
}

impl<T> ChunkLayer<Option<T>> {
    pub fn get(&self, coords: Vector2<i32>) -> Option<&T> {
        self.get_slot(coords).and_then(Option::as_ref)
    }

    pub fn get_mut(&mut self, coords: Vector2<i32>) -> Option<&mut T> {
        self.get_slot_mut(coords).and_then(Option::as_mut)
    }

    pub fn get_n_mut<const N: usize>(&mut self, coords: [Vector2<i32>; N]) -> [Option<&mut T>; N] {
        self.get_n_slots_mut(coords)
            .map(|opt| opt.and_then(Option::as_mut))
    }

    pub fn get_all_n_mut<const N: usize>(
        &mut self,
        coords: [Vector2<i32>; N],
    ) -> Option<[&mut T; N]> {
        let slots = self.get_all_n_slots_mut(coords)?;
        slots
            .iter()
            .all(|t| t.is_some())
            .then(|| slots.map(|opt| opt.as_mut().unwrap()))
    }

    pub fn insert(&mut self, coords: Vector2<i32>, value: T) -> Option<T> {
        let divided = DividedCoords::from_world_coords(coords);
        self.chunks.entry(divided.chunk_coords).or_default()[divided.sub_coords].replace(value)
    }

    pub fn remove(&mut self, coords: Vector2<i32>) -> Option<T> {
        self.get_slot_mut(coords).and_then(Option::take)
    }
}

#[derive(Debug, Clone)]
pub struct ChunkMap<T> {
    layers: BTreeMap<i32, ChunkLayer<T>>,
}

impl<T> Default for ChunkMap<T> {
    fn default() -> Self {
        Self {
            layers: BTreeMap::new(),
        }
    }
}

impl<T> ChunkMap<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_layers(layers: impl IntoIterator<Item = (i32, ChunkLayer<T>)>) -> Self {
        Self {
            layers: layers.into_iter().collect(),
        }
    }

    pub fn layers(&self) -> impl Iterator<Item = (i32, &ChunkLayer<T>)> {
        self.layers.iter().map(|(&i, c)| (i, c))
    }

    pub fn layers_mut(&mut self) -> impl Iterator<Item = (i32, &mut ChunkLayer<T>)> {
        self.layers.iter_mut().map(|(&i, c)| (i, c))
    }

    pub fn get_layers_in_range(
        &self,
        range: impl RangeBounds<i32>,
    ) -> impl Iterator<Item = (i32, &ChunkLayer<T>)> {
        self.layers.range(range).map(|(&i, c)| (i, c))
    }

    pub fn get_layers_in_range_mut(
        &mut self,
        range: impl RangeBounds<i32>,
    ) -> impl Iterator<Item = (i32, &mut ChunkLayer<T>)> {
        self.layers.range_mut(range).map(|(&i, c)| (i, c))
    }

    pub fn get_or_insert_layer(&mut self, index: i32) -> &mut ChunkLayer<T> {
        self.layers.entry(index).or_default()
    }

    pub fn insert_layer(&mut self, index: i32, layer: ChunkLayer<T>) -> Option<ChunkLayer<T>> {
        self.layers.insert(index, layer)
    }

    pub fn remove_layer(&mut self, index: i32) -> Option<ChunkLayer<T>> {
        self.layers.remove(&index)
    }

    pub fn get_slot(&self, coords: Vector3<i32>) -> Option<&T> {
        self.layers
            .get(&coords.z)
            .and_then(|layer| layer.get_slot(coords.xy()))
    }

    pub fn get_slot_mut(&mut self, coords: Vector3<i32>) -> Option<&mut T> {
        self.layers
            .get_mut(&coords.z)
            .and_then(|layer| layer.get_slot_mut(coords.xy()))
    }

    pub fn get_n_slots_mut<const N: usize>(
        &mut self,
        coords: [Vector3<i32>; N],
    ) -> [Option<&mut T>; N] {
        // Fast if N is small.
        for i in 0..N {
            for j in i..N {
                assert_ne!(
                    coords[i], coords[j],
                    "cannot mutably borrow the same slot twice!"
                );
            }
        }

        let mut ptrs = [None; N];
        for i in 0..N {
            let t = self.get_slot_mut(coords[i]);
            ptrs[i] = t.map(|mut_ref| NonNull::new(mut_ref as *mut T).unwrap());
        }

        // Safe: these all point to different elements.
        ptrs.map(|t| t.map(|mut nn| unsafe { nn.as_mut() }))
    }

    pub fn get_all_n_slots_mut<const N: usize>(
        &mut self,
        coords: [Vector3<i32>; N],
    ) -> Option<[&mut T; N]> {
        let slots = self.get_n_slots_mut(coords);

        if slots.iter().all(Option::is_some) {
            Some(slots.map(Option::unwrap))
        } else {
            None
        }
    }
}

impl<T> ChunkMap<Option<T>> {
    pub fn get(&self, coords: Vector3<i32>) -> Option<&T> {
        self.get_slot(coords).and_then(Option::as_ref)
    }

    pub fn get_mut(&mut self, coords: Vector3<i32>) -> Option<&mut T> {
        self.get_slot_mut(coords).and_then(Option::as_mut)
    }

    pub fn get_n_mut<const N: usize>(&mut self, coords: [Vector3<i32>; N]) -> [Option<&mut T>; N] {
        self.get_n_slots_mut(coords)
            .map(|opt| opt.and_then(Option::as_mut))
    }

    pub fn get_all_n_mut<const N: usize>(
        &mut self,
        coords: [Vector3<i32>; N],
    ) -> Option<[&mut T; N]> {
        let slots = self.get_all_n_slots_mut(coords)?;
        slots
            .iter()
            .all(|t| t.is_some())
            .then(|| slots.map(|opt| opt.as_mut().unwrap()))
    }

    pub fn insert(&mut self, coords: Vector3<i32>, value: T) -> Option<T> {
        self.layers
            .entry(coords.z)
            .or_default()
            .insert(coords.xy(), value)
    }

    pub fn remove(&mut self, coords: Vector3<i32>) -> Option<T> {
        self.get_slot_mut(coords).and_then(Option::take)
    }
}
