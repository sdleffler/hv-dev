use std::{
    collections::{BTreeMap, HashMap},
    mem::MaybeUninit,
    ops::{Deref, Index, IndexMut, RangeBounds},
    ptr::NonNull,
};

use bitvec::{array::BitArray, BitArr};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Hash)]
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

    pub fn new_unchecked(v: Vector2<u32>) -> Self {
        Self(v)
    }

    pub fn to_linear(self) -> usize {
        let v = self.cast::<usize>();
        v.y * CHUNK_SIDE_LENGTH + v.x
    }

    pub fn from_linear(linear: usize) -> Self {
        Self::new(
            Vector2::new(
                linear.rem_euclid(CHUNK_SIDE_LENGTH),
                linear.div_euclid(CHUNK_SIDE_LENGTH),
            )
            .cast::<u32>(),
        )
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

// Can't #[derive] on something w/ a type macro in it. (?? what?)
type ValidBits = BitArr!(for 256);

#[derive(Debug)]
pub struct Chunk<T> {
    data: Box<[MaybeUninit<T>; CHUNK_AREA]>,
    valid: ValidBits,
}

impl<T: Clone> Clone for Chunk<T> {
    fn clone(&self) -> Self {
        let mut new_chunk = Chunk::default();
        for index in self.valid.iter_ones() {
            new_chunk.data[index].write(unsafe { self.data[index].assume_init_ref() }.clone());
        }
        new_chunk.valid = self.valid;
        new_chunk
    }
}

impl<T> Index<SubCoords> for Chunk<T> {
    type Output = T;

    fn index(&self, index: SubCoords) -> &Self::Output {
        let linear = index.to_linear();
        assert!(self.valid[linear], "no initialized data at index!");
        unsafe { self.data[linear].assume_init_ref() }
    }
}

impl<T> IndexMut<SubCoords> for Chunk<T> {
    fn index_mut(&mut self, index: SubCoords) -> &mut Self::Output {
        let linear = index.to_linear();
        assert!(self.valid[linear], "no initialized data at index!");
        unsafe { self.data[linear].assume_init_mut() }
    }
}

impl<T> Default for Chunk<T> {
    fn default() -> Self {
        Chunk {
            data: Box::new(MaybeUninit::uninit_array()),
            valid: BitArray::zeroed(),
        }
    }
}

impl<T> Chunk<T> {
    pub fn get(&self, index: SubCoords) -> Option<&T> {
        let linear = index.to_linear();
        self.valid[linear].then(|| unsafe { self.data[linear].assume_init_ref() })
    }

    pub fn get_mut(&mut self, index: SubCoords) -> Option<&mut T> {
        let linear = index.to_linear();
        self.valid[linear].then(|| unsafe { self.data[linear].assume_init_mut() })
    }

    pub fn insert(&mut self, index: SubCoords, val: T) -> Option<T> {
        let linear = index.to_linear();
        match self.valid[linear] {
            true => Some(std::mem::replace(
                unsafe { self.data[linear].assume_init_mut() },
                val,
            )),
            false => {
                self.valid.set(linear, true);
                self.data[linear].write(val);
                None
            }
        }
    }

    pub fn remove(&mut self, index: SubCoords) -> Option<T> {
        let linear = index.to_linear();
        self.valid[linear].then(|| {
            self.valid.set(linear, false);
            unsafe { std::ptr::read(&self.data[linear]).assume_init() }
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = (SubCoords, &T)> + ExactSizeIterator {
        self.valid.iter_ones().map(|i| {
            (SubCoords::from_linear(i), unsafe {
                self.data[i].assume_init_ref()
            })
        })
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (SubCoords, &mut T)> + ExactSizeIterator {
        self.valid.iter_ones().map(|i| {
            // safety: we're continuously borrowing different elements, and we have mutable access;
            // so no one else is going to try to access the same two, and we're assured we won't try
            // to access the same two since our valid bits won't repeat.
            (SubCoords::from_linear(i), unsafe {
                (*(&mut self.data[i] as *mut MaybeUninit<_>)).assume_init_mut()
            })
        })
    }

    /// Clear the chunk.
    pub fn clear(&mut self) {
        // TODO: most cases should not need to run a destructor on the elements, but, for
        // completeness and correctness, it should be done.
        self.valid.set_all(false);
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

    pub fn chunks(&self) -> impl Iterator<Item = (ChunkCoords, &Chunk<T>)> {
        self.chunks.iter().map(|(&coords, chunk)| (coords, chunk))
    }

    pub fn chunks_mut(&mut self) -> impl Iterator<Item = (ChunkCoords, &mut Chunk<T>)> {
        self.chunks
            .iter_mut()
            .map(|(&coords, chunk)| (coords, chunk))
    }

    pub fn get_chunk(&self, coords: ChunkCoords) -> Option<&Chunk<T>> {
        self.chunks.get(&coords)
    }

    pub fn get_chunk_mut(&mut self, coords: ChunkCoords) -> Option<&mut Chunk<T>> {
        self.chunks.get_mut(&coords)
    }

    pub fn get_or_insert_chunk(&mut self, coords: ChunkCoords) -> &mut Chunk<T> {
        self.chunks.entry(coords).or_default()
    }

    pub fn insert_chunk(&mut self, coords: ChunkCoords, chunk: Chunk<T>) -> Option<Chunk<T>> {
        self.chunks.insert(coords, chunk)
    }

    pub fn remove_chunk(&mut self, coords: ChunkCoords) -> Option<Chunk<T>> {
        self.chunks.remove(&coords)
    }

    pub fn get(&self, coords: Vector2<i32>) -> Option<&T> {
        let divided = DividedCoords::from_world_coords(coords);
        self.get_chunk(divided.chunk_coords)
            .and_then(|chunk| chunk.get(divided.sub_coords))
    }

    pub fn get_mut(&mut self, coords: Vector2<i32>) -> Option<&mut T> {
        let divided = DividedCoords::from_world_coords(coords);
        self.get_chunk_mut(divided.chunk_coords)
            .and_then(|chunk| chunk.get_mut(divided.sub_coords))
    }

    pub fn get_n_mut<const N: usize>(&mut self, coords: [Vector2<i32>; N]) -> [Option<&mut T>; N] {
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
            let t = self.get_mut(coords[i]);
            ptrs[i] = t.map(|mut_ref| NonNull::new(mut_ref as *mut T).unwrap());
        }

        // Safe: these all point to different elements.
        ptrs.map(|t| t.map(|mut nn| unsafe { nn.as_mut() }))
    }

    pub fn get_all_n_mut<const N: usize>(
        &mut self,
        coords: [Vector2<i32>; N],
    ) -> Option<[&mut T; N]> {
        let slots = self.get_n_mut(coords);

        if slots.iter().all(Option::is_some) {
            Some(slots.map(Option::unwrap))
        } else {
            None
        }
    }

    pub fn insert(&mut self, coords: Vector2<i32>, value: T) -> Option<T> {
        let divided = DividedCoords::from_world_coords(coords);
        self.chunks
            .entry(divided.chunk_coords)
            .or_default()
            .insert(divided.sub_coords, value)
    }

    pub fn remove(&mut self, coords: Vector2<i32>) -> Option<T> {
        let divided = DividedCoords::from_world_coords(coords);
        self.get_chunk_mut(divided.chunk_coords)
            .and_then(|chunk| chunk.remove(divided.sub_coords))
    }

    pub fn iter(&self) -> impl Iterator<Item = (Vector2<i32>, &T)> {
        self.chunks().flat_map(|(chunk_coords, chunk)| {
            chunk.iter().map(move |(sub_coords, value)| {
                (
                    DividedCoords {
                        chunk_coords,
                        sub_coords,
                    }
                    .to_world_coords(),
                    value,
                )
            })
        })
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Vector2<i32>, &mut T)> {
        self.chunks_mut().flat_map(|(chunk_coords, chunk)| {
            chunk.iter_mut().map(move |(sub_coords, value)| {
                (
                    DividedCoords {
                        chunk_coords,
                        sub_coords,
                    }
                    .to_world_coords(),
                    value,
                )
            })
        })
    }

    pub fn clear(&mut self) {
        self.chunks.values_mut().for_each(Chunk::clear);
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

    pub fn get_layer(&self, index: i32) -> Option<&ChunkLayer<T>> {
        self.layers.get(&index)
    }

    pub fn get_layer_mut(&mut self, index: i32) -> Option<&mut ChunkLayer<T>> {
        self.layers.get_mut(&index)
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

    pub fn get(&self, coords: Vector3<i32>) -> Option<&T> {
        self.layers
            .get(&coords.z)
            .and_then(|layer| layer.get(coords.xy()))
    }

    pub fn get_mut(&mut self, coords: Vector3<i32>) -> Option<&mut T> {
        self.layers
            .get_mut(&coords.z)
            .and_then(|layer| layer.get_mut(coords.xy()))
    }

    pub fn get_n_mut<const N: usize>(&mut self, coords: [Vector3<i32>; N]) -> [Option<&mut T>; N] {
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
            let t = self.get_mut(coords[i]);
            ptrs[i] = t.map(|mut_ref| NonNull::new(mut_ref as *mut T).unwrap());
        }

        // Safe: these all point to different elements.
        ptrs.map(|t| t.map(|mut nn| unsafe { nn.as_mut() }))
    }

    pub fn get_all_n_mut<const N: usize>(
        &mut self,
        coords: [Vector3<i32>; N],
    ) -> Option<[&mut T; N]> {
        let slots = self.get_n_mut(coords);

        if slots.iter().all(Option::is_some) {
            Some(slots.map(Option::unwrap))
        } else {
            None
        }
    }

    pub fn insert(&mut self, coords: Vector3<i32>, value: T) -> Option<T> {
        self.layers
            .entry(coords.z)
            .or_default()
            .insert(coords.xy(), value)
    }

    pub fn remove(&mut self, coords: Vector3<i32>) -> Option<T> {
        self.layers
            .get_mut(&coords.z)
            .and_then(|layer| layer.remove(coords.xy()))
    }

    pub fn iter(&self) -> impl Iterator<Item = (Vector3<i32>, &T)> {
        self.layers()
            .flat_map(|(z, layer)| layer.iter().map(move |(xy, value)| (xy.push(z), value)))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Vector3<i32>, &mut T)> {
        self.layers_mut()
            .flat_map(|(z, layer)| layer.iter_mut().map(move |(xy, value)| (xy.push(z), value)))
    }

    /// Clear the map while retaining the allocated memory.
    pub fn clear(&mut self) {
        self.layers.values_mut().for_each(ChunkLayer::clear);
    }
}
