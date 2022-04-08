//! # Heavy Stampede - a friendly herd of `bumpalo`s, with extra features
//!
//! This crate builds on and reexports most things from the [`bumpalo`] crate. In implementation and
//! usage, it is very similar to the `bumpalo-herd` crate, but supports some operations that `Herd`
//! does not:
//!
//! - [`BumpPool`] supports temporarily detaching a [`Bump`] from the pool through
//!   [`PooledBump::detach`], locking it to the thread it was detached in until it is dropped and
//!   returned to the pool.
//! - [`PooledBump::as_bump_unbound`], unsafe access to the managed [`Bump`] inside.

#![feature(maybe_uninit_slice)]
#![no_std]

use core::{
    mem::{ManuallyDrop, MaybeUninit},
    ops::{Deref, DerefMut},
    pin::Pin,
};

use alloc::{boxed::Box, vec::Vec};
use spin::Mutex;

extern crate alloc;

pub use bumpalo::{
    boxed, collections, format, vec, AllocOrInitError, Bump, ChunkIter, ChunkRawIter,
};

/// A thread-safe pool of [`Bump`]s (provided as [`PooledBump`]s). Internally, this is two pools;
/// the "ready" pool of [`Bump`]s which do not yet have any objects tied to their lifetimes, and the
/// "shunned" pool of [`Bump`]s which were detached using [`PooledBump::detach`] and must not be
/// allocated from or destroyed until the entire pool is [`.reset()`](BumpPool::reset).
pub struct BumpPool {
    // The pool of `Bump`s which can be immediately used.
    ready: Mutex<Vec<Pin<Box<Bump>>>>,
    // The pool of `Bump`s which have been used thread-locally as allocators and can no longer be
    // shared. Returns to `ready` after a `reset` call.
    shunned: Mutex<Vec<Pin<Box<Bump>>>>,
}

impl BumpPool {
    /// Create an empty [`BumpPool`].
    pub const fn new() -> Self {
        Self {
            ready: Mutex::new(Vec::new()),
            shunned: Mutex::new(Vec::new()),
        }
    }

    /// Reset the [`BumpPool`], returning any "shunned" allocators to the "ready" pool and resetting
    /// all pooled allocators.
    pub fn reset(&mut self) {
        for pool in self.shunned.get_mut().drain(..) {
            self.ready.get_mut().push(pool);
        }
        self.ready.get_mut().iter_mut().for_each(|b| b.reset());
    }

    /// Get a [`Bump`] from the pool, or allocate a new [`Bump`] if the pool is empty.
    pub fn get(&self) -> PooledBump {
        let next = self
            .ready
            .lock()
            .pop()
            .unwrap_or_else(|| Box::pin(Bump::new()));
        PooledBump {
            stampede: self,
            bump: ManuallyDrop::new(next),
        }
    }
}

/// A [`Bump`] which was allocated from a [`BumpPool`].
pub struct PooledBump<'s> {
    stampede: &'s BumpPool,
    bump: ManuallyDrop<Pin<Box<Bump>>>,
}

impl<'s> PooledBump<'s> {
    pub fn alloc<T>(&self, val: T) -> &'s mut T {
        unsafe { self.as_bump_unbound().alloc(val) }
    }

    pub fn alloc_boxed<T>(&self, val: T) -> boxed::Box<'s, T> {
        boxed::Box::new_in(val, unsafe { self.as_bump_unbound() })
    }

    pub fn chunk<T>(&self, size: usize) -> Chunk<'s, T> {
        Chunk::new(unsafe {
            self.as_bump_unbound()
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
    pub unsafe fn as_bump_unbound(&self) -> &'s Bump {
        let bump_ref = self.bump.as_ref().get_ref();
        core::mem::transmute::<&Bump, &'s Bump>(bump_ref)
    }

    /// Get access to the [`Bump`] inside without allowing the reference to outlive the
    /// `PooledBump`. It's important to note that objects allocated inside the returned `&Bump` will
    /// not be able to live for as long as `'s`/the lifetime parameter of the `PooledBump`. This is
    /// because if used as an `Allocator`, `&'s Bump` from a `PooledBump<'s>` will provide the `'s`
    /// lifetime to objects such as [`alloc::boxed::Box`], which means they can live past the
    /// lifetime of the `PooledBump`, the `PooledBump` is pulled from the pool into another thread,
    /// the box deallocates, and then calls the `.dealloc` fn on the `PooledBump`... which is in
    /// another thread, could be trying to allocate/etc., and runs into a race condition.
    pub fn as_bump(&self) -> &Bump {
        self.bump.as_ref().get_ref()
    }

    /// Consumes the `PooledBump` and temporarily "shuns" the [`Bump`], placing it into a special
    /// pool inside the [`BumpPool`] which contains pools that are now detached and floating in a
    /// thread without being `Sync` but which can be returned to the "ready" pool once the entire
    /// [`BumpPool`] is reset. Unlike [`PooledBump::as_bump_unbound`], the returned `&'s Bump` is
    /// completely safe to use, including as an allocator - because the type `&Bump` is non-`Send`,
    /// it's now permanently locked to the thread it was detached in, and cannot be accessed from
    /// another thread. That is, until its borrow ends, the [`BumpPool`] is reset, and it returns to
    /// the "ready" pool having been reset itself.
    pub fn detach(mut self) -> &'s Bump {
        let bump = unsafe { self.as_bump_unbound() };
        let mut shunned = self.stampede.shunned.lock();
        shunned.push(unsafe { ManuallyDrop::take(&mut self.bump) });
        core::mem::forget(self);
        bump
    }
}

impl<'s> Drop for PooledBump<'s> {
    fn drop(&mut self) {
        let mut herd = self.stampede.ready.lock();
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
