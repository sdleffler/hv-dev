use std::collections::HashMap;

use crate::lattice::{ChunkCoords, SubCoords};

#[derive(Debug, Clone, Copy)]
pub enum SlotEventKind<T: Copy> {
    Insert { new: T, prev: Option<T> },
    Remove { prev: T },
}

impl<T: Copy> SlotEventKind<T> {
    pub fn after(self) -> Option<T> {
        match self {
            Self::Insert { new, .. } => Some(new),
            Self::Remove { .. } => None,
        }
    }

    pub fn before(self) -> Option<T> {
        match self {
            Self::Insert { prev, .. } => prev,
            Self::Remove { prev } => Some(prev),
        }
    }

    pub fn append(self, new: Self) -> Option<Self> {
        use SlotEventKind::*;
        match (self, new) {
            (Insert { prev, .. }, Insert { new, .. }) => Some(Insert { new, prev }),
            // An insert from nothing to a remove cancel each other out.
            (Insert { prev, .. }, Remove { .. }) => prev.map(|prev| Remove { prev }),
            (Remove { prev }, Insert { new, .. }) => Some(Insert {
                new,
                prev: Some(prev),
            }),
            (Remove { .. }, Remove { .. }) => unreachable!(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SlotEvent<T: Copy> {
    pub layer: i32,
    pub chunk: ChunkCoords,
    pub sub: SubCoords,
    pub kind: SlotEventKind<T>,
}

#[derive(Debug, Clone, Copy)]
pub enum ChunkEventKind {
    Insert,
    Remove,
}

#[derive(Debug, Clone, Copy)]
pub struct ChunkEvent {
    pub layer: i32,
    pub chunk: ChunkCoords,
    pub kind: ChunkEventKind,
}

#[derive(Debug, Clone, Copy)]
pub enum LayerEventKind {
    Insert,
    Remove,
}

#[derive(Debug, Clone, Copy)]
pub struct LayerEvent {
    pub layer: i32,
    pub kind: LayerEventKind,
}

#[derive(Debug, Clone, Copy)]
pub enum LatticeEvent<T: Copy> {
    Slot(SlotEvent<T>),
    Chunk(ChunkEvent),
    Layer(LayerEvent),
}

#[derive(Debug)]
struct ChunkEventDebouncer<T: Copy> {
    chunk_event: Option<ChunkEventKind>,
    per_slot: HashMap<SubCoords, SlotEventKind<T>>,
}

impl<T: Copy> Default for ChunkEventDebouncer<T> {
    fn default() -> Self {
        Self {
            chunk_event: None,
            per_slot: HashMap::new(),
        }
    }
}

impl<T: Copy> ChunkEventDebouncer<T> {
    fn push_slot_event(&mut self, ev: SlotEvent<T>) {
        use std::collections::hash_map::Entry::*;
        let entry = self.per_slot.entry(ev.sub);
        match entry {
            Occupied(mut occupied) => {
                match occupied.get().append(ev.kind) {
                    Some(new) => occupied.insert(new),
                    None => occupied.remove(),
                };
            }
            Vacant(vacant) => {
                vacant.insert(ev.kind);
            }
        }
    }
}

#[derive(Debug)]
struct LayerEventDebouncer<T: Copy> {
    layer_event: Option<LayerEventKind>,
    per_chunk: HashMap<ChunkCoords, ChunkEventDebouncer<T>>,
}

impl<T: Copy> Default for LayerEventDebouncer<T> {
    fn default() -> Self {
        Self {
            layer_event: None,
            per_chunk: HashMap::new(),
        }
    }
}

impl<T: Copy> LayerEventDebouncer<T> {
    fn push_chunk_event(&mut self, ev: ChunkEvent) {
        let ced = self.per_chunk.entry(ev.chunk).or_default();
        ced.per_slot.clear();
        ced.chunk_event = Some(ev.kind);
    }
}

#[derive(Debug)]
pub struct LatticeEventDebouncer<T: Copy> {
    per_layer: HashMap<i32, LayerEventDebouncer<T>>,
}

impl<T: Copy> Default for LatticeEventDebouncer<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy> LatticeEventDebouncer<T> {
    pub fn new() -> Self {
        Self {
            per_layer: HashMap::new(),
        }
    }

    fn push_layer_event(&mut self, ev: LayerEvent) {
        let led = self.per_layer.entry(ev.layer).or_default();
        led.per_chunk.clear();
        led.layer_event = Some(ev.kind);
    }

    pub fn push(&mut self, event: LatticeEvent<T>) {
        match event {
            LatticeEvent::Layer(layer_event) => self.push_layer_event(layer_event),
            LatticeEvent::Chunk(chunk_event) => self
                .per_layer
                .entry(chunk_event.layer)
                .or_default()
                .push_chunk_event(chunk_event),
            LatticeEvent::Slot(slot_event) => self
                .per_layer
                .entry(slot_event.layer)
                .or_default()
                .per_chunk
                .entry(slot_event.chunk)
                .or_default()
                .push_slot_event(slot_event),
        }
    }

    /// Returns all debounced events, in order of layer events first, then, per chunk, a chunk
    /// event for that chunk (if it exists) followed by slot events for that chunk.
    pub fn drain(&mut self) -> impl Iterator<Item = LatticeEvent<T>> + '_ {
        self.per_layer.iter_mut().flat_map(|(&layer, led)| {
            let maybe_event = led
                .layer_event
                .take()
                .map(|kind| LayerEvent { layer, kind })
                .into_iter();
            let per_chunk = led.per_chunk.iter_mut().flat_map(move |(&chunk, ced)| {
                let maybe_event = ced
                    .chunk_event
                    .take()
                    .map(|kind| ChunkEvent { layer, chunk, kind })
                    .into_iter();
                let per_slot = ced.per_slot.drain().map(move |(sub, kind)| SlotEvent {
                    layer,
                    chunk,
                    sub,
                    kind,
                });
                maybe_event
                    .map(LatticeEvent::Chunk)
                    .chain(per_slot.map(LatticeEvent::Slot))
            });
            maybe_event.map(LatticeEvent::Layer).chain(per_chunk)
        })
    }
}

impl<T: Copy> Extend<LatticeEvent<T>> for LatticeEventDebouncer<T> {
    fn extend<I: IntoIterator<Item = LatticeEvent<T>>>(&mut self, iter: I) {
        for event in iter {
            self.push(event);
        }
    }
}
