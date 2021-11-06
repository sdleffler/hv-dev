#![feature(allocator_api)]
#![feature(maybe_uninit_slice, maybe_uninit_extra)]
#![no_std]

use core::{
    alloc::{AllocError, Allocator, Layout},
    mem::{ManuallyDrop, MaybeUninit},
    ops::{Deref, DerefMut},
    pin::Pin,
    ptr::NonNull,
};

use alloc::{boxed::Box, vec::Vec};
use spin::Mutex;

extern crate alloc;

pub use bumpalo::{boxed::Box as Owned, AllocOrInitError, Bump, ChunkIter, ChunkRawIter};

pub struct Stampede {
    herd: Mutex<Vec<Pin<Box<Bump>>>>,
}

impl Stampede {
    pub const fn new() -> Self {
        Self {
            herd: Mutex::new(Vec::new()),
        }
    }

    pub fn reset(&mut self) {
        self.herd.get_mut().iter_mut().for_each(|b| b.reset());
    }

    pub fn get(&self) -> PooledBump {
        let next = self
            .herd
            .lock()
            .pop()
            .unwrap_or_else(|| Box::pin(Bump::new()));
        PooledBump {
            stampede: self,
            bump: ManuallyDrop::new(next),
        }
    }
}

pub struct PooledBump<'s> {
    stampede: &'s Stampede,
    bump: ManuallyDrop<Pin<Box<Bump>>>,
}

unsafe impl<'s> Allocator for PooledBump<'s> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        unsafe { self.as_bump().allocate(layout) }
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        self.as_bump().deallocate(ptr, layout)
    }

    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        self.as_bump().shrink(ptr, old_layout, new_layout)
    }

    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        self.as_bump().grow(ptr, old_layout, new_layout)
    }

    unsafe fn grow_zeroed(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        self.as_bump().grow_zeroed(ptr, old_layout, new_layout)
    }
}

impl<'s> PooledBump<'s> {
    pub fn alloc<T>(&self, val: T) -> &'s mut T {
        unsafe { self.as_bump().alloc(val) }
    }

    pub fn alloc_boxed<T>(&self, val: T) -> Owned<'s, T> {
        Owned::new_in(val, unsafe { self.as_bump() })
    }

    pub fn chunk<T>(&self, size: usize) -> Chunk<'s, T> {
        Chunk::new(unsafe {
            self.as_bump()
                .alloc_slice_fill_with(size, |_| MaybeUninit::uninit())
        })
    }

    /// This function is unsafe because of the lifetimes involved: the bump arena itself must not
    /// outlive the [`PooledBump`] it came from.
    ///
    /// # Safety
    ///
    /// You should only ever use this as a temp variable/middle step when allocating something
    /// inside the `PooledBump`.
    pub unsafe fn as_bump(&self) -> &'s Bump {
        let bump_ref = self.bump.as_ref().get_ref();
        core::mem::transmute::<&Bump, &'s Bump>(bump_ref)
    }
}

impl<'s> Drop for PooledBump<'s> {
    fn drop(&mut self) {
        let mut herd = self.stampede.herd.lock();
        herd.push(unsafe { ManuallyDrop::take(&mut self.bump) });
    }
}

pub struct Chunk<'s, T> {
    len: usize,
    storage: &'s mut [MaybeUninit<T>],
}

impl<'s, T> Deref for Chunk<'s, T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        unsafe { MaybeUninit::slice_assume_init_ref(&self.storage[..self.len]) }
    }
}

impl<'s, T> DerefMut for Chunk<'s, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { MaybeUninit::slice_assume_init_mut(&mut self.storage[..self.len]) }
    }
}

impl<'s, T> Chunk<'s, T> {
    pub fn new(storage: &'s mut [MaybeUninit<T>]) -> Self {
        Self { len: 0, storage }
    }

    pub fn push(&mut self, val: T) -> Result<(), T> {
        if self.len < self.capacity() {
            self.storage[self.len].write(val);
            Ok(())
        } else {
            Err(val)
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn capacity(&self) -> usize {
        self.storage.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl<'s, T> Drop for Chunk<'s, T> {
    fn drop(&mut self) {
        unsafe {
            for i in 0..self.len {
                self.storage[i].assume_init_drop();
            }
        }
    }
}

pub struct ChunkIntoIter<'s, T> {
    storage: &'s mut [MaybeUninit<T>],
}

impl<'s, T> Iterator for ChunkIntoIter<'s, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let storage = core::mem::take(&mut self.storage);
        let (head, new_tail) = storage.split_first_mut()?;
        self.storage = new_tail;
        Some(unsafe { head.assume_init_read() })
    }
}

impl<'s, T> Drop for ChunkIntoIter<'s, T> {
    fn drop(&mut self) {
        self.for_each(drop);
    }
}

impl<'s, T> IntoIterator for Chunk<'s, T> {
    type Item = T;
    type IntoIter = ChunkIntoIter<'s, T>;

    fn into_iter(mut self) -> Self::IntoIter {
        let storage = core::mem::take(&mut self.storage);
        self.len = 0;
        ChunkIntoIter { storage }
    }
}
