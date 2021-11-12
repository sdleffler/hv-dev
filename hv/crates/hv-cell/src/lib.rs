/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! A `no_std` port of the `atomic_refcell` crate, with added functionality for `Arc`-wrapped
//! `AtomicRefCell`s.
//!
//! Implements a container type providing RefCell-like semantics for objects
//! shared across threads.
//!
//! RwLock is traditionally considered to be the |Sync| analogue of RefCell.
//! However, for consumers that can guarantee that they will never mutably
//! borrow the contents concurrently with immutable borrows, an RwLock is
//! overkill, and has key disadvantages:
//! * Performance: Even the fastest existing implementation of RwLock (that of
//!   parking_lot) performs at least two atomic operations during immutable
//!   borrows. This makes mutable borrows significantly cheaper than immutable
//!   borrows, leading to weird incentives when writing performance-critical
//!   code.
//! * Features: Implementing AtomicRefCell on top of RwLock makes it impossible
//!   to implement useful things like AtomicRef{,Mut}::map.
//!
//! As such, we re-implement RefCell semantics from scratch with a single atomic
//! reference count. The primary complication of this scheme relates to keeping
//! things in a consistent state when one thread performs an illegal borrow and
//! panics. Since an AtomicRefCell can be accessed by multiple threads, and since
//! panics are recoverable, we need to ensure that an illegal (panicking) access by
//! one thread does not lead to undefined behavior on other, still-running threads.
//!
//! So we represent things as follows:
//! * Any value with the high bit set (so half the total refcount space) indicates
//!   a mutable borrow.
//! * Mutable borrows perform an atomic compare-and-swap, swapping in the high bit
//!   if the current value is zero. If the current value is non-zero, the thread
//!   panics and the value is left undisturbed.
//! * Immutable borrows perform an atomic increment. If the new value has the high
//!   bit set, the thread panics. The incremented refcount is left as-is, since it
//!   still represents a valid mutable borrow. When the mutable borrow is released,
//!   the refcount is set unconditionally to zero, clearing any stray increments by
//!   panicked threads.
//!
//! There are a few additional purely-academic complications to handle overflow,
//! which are documented in the implementation.
//!
//! The rest of this module is mostly derived by copy-pasting the implementation of
//! RefCell and fixing things up as appropriate. Certain non-threadsafe methods
//! have been removed. We segment the concurrency logic from the rest of the code to
//! keep the tricky parts small and easy to audit.

#![no_std]
#![feature(generic_associated_types)]
#![allow(unsafe_code)]
#![deny(missing_docs)]

extern crate alloc;

use alloc::sync::Arc;
use core::cmp;
use core::fmt;
use core::fmt::{Debug, Display};
use core::ops::{Deref, DerefMut};
use core::sync::atomic;
use core::sync::atomic::AtomicUsize;
use core::{cell::UnsafeCell, convert::Infallible};
use hv_guarded_borrow::{
    NonBlockingGuardedBorrow, NonBlockingGuardedBorrowMut, NonBlockingGuardedMutBorrowMut,
};

#[cfg(feature = "track-leases")]
use hv_lease_tracker::{Lease, LeaseTracker};

/// A threadsafe analogue to RefCell but where the borrows are considered strong references to the
/// inner `Arc`'d value.
pub struct ArcCell<T: ?Sized> {
    inner: Arc<AtomicRefCell<T>>,
}

impl<T> ArcCell<T> {
    /// Wrap a value in an `ArcCell`.
    #[inline]
    pub fn new(value: T) -> Self {
        Self {
            inner: Arc::new(AtomicRefCell::new(value)),
        }
    }
}

impl<T: ?Sized> ArcCell<T> {
    /// Immutably borrows the wrapped value.
    #[inline]
    pub fn borrow(&self) -> ArcRef<T> {
        match AtomicBorrowRef::try_new(&self.inner.borrows) {
            Ok(borrow) => ArcRef {
                value: unsafe { &*(*self.inner).value.get() },
                guard: ArcRefGuard {
                    borrow,
                    cell: self.inner.clone(),
                },

                #[cfg(feature = "track-leases")]
                lease: self.inner.lease_tracker.lease_at_caller(Some("immutable")),
            },
            Err(s) => panic!("{}", s),
        }
    }

    /// Attempts to immutably borrow the wrapped value, but instead of panicking
    /// on a failed borrow, returns `Err`.
    #[inline]
    pub fn try_borrow(&self) -> Result<ArcRef<T>, BorrowError> {
        match AtomicBorrowRef::try_new(&self.inner.borrows) {
            Ok(borrow) => Ok(ArcRef {
                value: unsafe { &*self.inner.value.get() },
                guard: ArcRefGuard {
                    borrow,
                    cell: self.inner.clone(),
                },

                #[cfg(feature = "track-leases")]
                lease: self.inner.lease_tracker.lease_at_caller(Some("immutable")),
            }),
            Err(_) => Err(BorrowError { _private: () }),
        }
    }

    /// Mutably borrows the wrapped value.
    #[inline]
    pub fn borrow_mut(&self) -> ArcRefMut<T> {
        match AtomicBorrowRefMut::try_new(&self.inner.borrows) {
            Ok(borrow) => ArcRefMut {
                value: unsafe { &mut *self.inner.value.get() },
                guard: ArcRefMutGuard {
                    cell: self.inner.clone(),
                    borrow,
                },

                #[cfg(feature = "track-leases")]
                lease: self.inner.lease_tracker.lease_at_caller(Some("mutable")),
            },
            Err(s) => panic!("{}", s),
        }
    }

    /// Attempts to mutably borrow the wrapped value, but instead of panicking
    /// on a failed borrow, returns `Err`.
    #[inline]
    pub fn try_borrow_mut(&self) -> Result<ArcRefMut<T>, BorrowMutError> {
        match AtomicBorrowRefMut::try_new(&self.inner.borrows) {
            Ok(borrow) => Ok(ArcRefMut {
                value: unsafe { &mut *self.inner.value.get() },
                guard: ArcRefMutGuard {
                    cell: self.inner.clone(),
                    borrow,
                },

                #[cfg(feature = "track-leases")]
                lease: self.inner.lease_tracker.lease_at_caller(Some("mutable")),
            }),
            Err(_) => Err(BorrowMutError { _private: () }),
        }
    }

    /// Get a reference to the [`Arc`]'d [`AtomicRefCell`] which is managed by this type.
    #[inline]
    pub fn as_inner(&self) -> &Arc<AtomicRefCell<T>> {
        &self.inner
    }

    /// Consume the `ArcCell` and return the inner [`Arc`]'d [`AtomicRefCell`] which is managed by
    /// this type.
    #[inline]
    pub fn into_inner(self) -> Arc<AtomicRefCell<T>> {
        self.inner
    }

    /// Construct an `ArcCell` from an [`Arc`]'d [`AtomicRefCell`].
    #[inline]
    pub fn from_inner(inner: Arc<AtomicRefCell<T>>) -> Self {
        Self { inner }
    }
}

/// A threadsafe analogue to RefCell.
pub struct AtomicRefCell<T: ?Sized> {
    #[cfg(feature = "track-leases")]
    lease_tracker: LeaseTracker,

    borrows: AtomicUsize,
    value: UnsafeCell<T>,
}

/// An error returned by [`AtomicRefCell::try_borrow`](struct.AtomicRefCell.html#method.try_borrow).
pub struct BorrowError {
    _private: (),
}

impl Debug for BorrowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BorrowError").finish()
    }
}

impl Display for BorrowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt("already mutably borrowed", f)
    }
}

/// An error returned by [`AtomicRefCell::try_borrow_mut`](struct.AtomicRefCell.html#method.try_borrow_mut).
pub struct BorrowMutError {
    _private: (),
}

impl Debug for BorrowMutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BorrowMutError").finish()
    }
}

impl Display for BorrowMutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt("already borrowed", f)
    }
}

impl<T> AtomicRefCell<T> {
    /// Creates a new `AtomicRefCell` containing `value`.
    #[inline]
    pub fn new(value: T) -> AtomicRefCell<T> {
        AtomicRefCell {
            #[cfg(feature = "track-leases")]
            lease_tracker: LeaseTracker::new(),

            borrows: AtomicUsize::new(0),
            value: UnsafeCell::new(value),
        }
    }

    /// Consumes the `AtomicRefCell`, returning the wrapped value.
    #[inline]
    pub fn into_inner(self) -> T {
        debug_assert!(self.borrows.load(atomic::Ordering::Acquire) == 0);
        self.value.into_inner()
    }
}

impl<T: ?Sized> AtomicRefCell<T> {
    /// Immutably borrows the wrapped value.
    #[inline]
    #[track_caller]
    pub fn borrow(&self) -> AtomicRef<T> {
        match AtomicBorrowRef::try_new(&self.borrows) {
            Ok(borrow) => AtomicRef {
                value: unsafe { &*self.value.get() },
                guard: AtomicRefGuard {
                    count: &self.borrows,
                    borrow,
                },

                #[cfg(feature = "track-leases")]
                lease: self.lease_tracker.lease_at_caller(Some("immutable")),
            },
            Err(s) => panic!("{}", s),
        }
    }

    /// Attempts to immutably borrow the wrapped value, but instead of panicking
    /// on a failed borrow, returns `Err`.
    #[inline]
    #[track_caller]
    pub fn try_borrow(&self) -> Result<AtomicRef<T>, BorrowError> {
        match AtomicBorrowRef::try_new(&self.borrows) {
            Ok(borrow) => Ok(AtomicRef {
                value: unsafe { &*self.value.get() },
                guard: AtomicRefGuard {
                    count: &self.borrows,
                    borrow,
                },

                #[cfg(feature = "track-leases")]
                lease: self.lease_tracker.lease_at_caller(Some("immutable")),
            }),
            Err(_) => Err(BorrowError { _private: () }),
        }
    }

    /// Mutably borrows the wrapped value.
    #[inline]
    #[track_caller]
    pub fn borrow_mut(&self) -> AtomicRefMut<T> {
        match AtomicBorrowRefMut::try_new(&self.borrows) {
            Ok(borrow) => AtomicRefMut {
                value: unsafe { &mut *self.value.get() },
                guard: AtomicRefMutGuard {
                    count: &self.borrows,
                    borrow,
                },

                #[cfg(feature = "track-leases")]
                lease: self.lease_tracker.lease_at_caller(Some("mutable")),
            },
            Err(s) => panic!("{}", s),
        }
    }

    /// Attempts to mutably borrow the wrapped value, but instead of panicking
    /// on a failed borrow, returns `Err`.
    #[inline]
    #[track_caller]
    pub fn try_borrow_mut(&self) -> Result<AtomicRefMut<T>, BorrowMutError> {
        match AtomicBorrowRefMut::try_new(&self.borrows) {
            Ok(borrow) => Ok(AtomicRefMut {
                value: unsafe { &mut *self.value.get() },
                guard: AtomicRefMutGuard {
                    count: &self.borrows,
                    borrow,
                },

                #[cfg(feature = "track-leases")]
                lease: self.lease_tracker.lease_at_caller(Some("mutable")),
            }),
            Err(_) => Err(BorrowMutError { _private: () }),
        }
    }

    /// Returns a raw pointer to the underlying data in this cell.
    ///
    /// External synchronization is needed to avoid data races when dereferencing
    /// the pointer.
    #[inline]
    pub fn as_ptr(&self) -> *mut T {
        self.value.get()
    }

    /// Returns a mutable reference to the wrapped value.
    ///
    /// No runtime checks take place (unless debug assertions are enabled)
    /// because this call borrows `AtomicRefCell` mutably at compile-time.
    #[inline]
    pub fn get_mut(&mut self) -> &mut T {
        debug_assert!(self.borrows.load(atomic::Ordering::Acquire) == 0);
        unsafe { &mut *self.value.get() }
    }
}

//
// Core synchronization logic. Keep this section small and easy to audit.
//

const HIGH_BIT: usize = !(::core::usize::MAX >> 1);
const MAX_FAILED_BORROWS: usize = HIGH_BIT + (HIGH_BIT >> 1);

struct AtomicBorrowRef;

impl AtomicBorrowRef {
    #[inline]
    fn try_new(borrow: &AtomicUsize) -> Result<Self, &'static str> {
        let new = borrow.fetch_add(1, atomic::Ordering::Acquire) + 1;
        if new & HIGH_BIT != 0 {
            // If the new count has the high bit set, that almost certainly
            // means there's an pre-existing mutable borrow. In that case,
            // we simply leave the increment as a benign side-effect and
            // return `Err`. Once the mutable borrow is released, the
            // count will be reset to zero unconditionally.
            //
            // The overflow check here ensures that an unbounded number of
            // immutable borrows during the scope of one mutable borrow
            // will soundly trigger a panic (or abort) rather than UB.
            Self::check_overflow(borrow, new);
            Err("already mutably borrowed")
        } else {
            Ok(AtomicBorrowRef)
        }
    }

    #[cold]
    #[inline(never)]
    fn check_overflow(borrow: &AtomicUsize, new: usize) {
        if new == HIGH_BIT {
            // We overflowed into the reserved upper half of the refcount
            // space. Before panicking, decrement the refcount to leave things
            // in a consistent immutable-borrow state.
            //
            // This can basically only happen if somebody forget()s AtomicRefs
            // in a tight loop.
            borrow.fetch_sub(1, atomic::Ordering::Release);
            panic!("too many immutable borrows");
        } else if new >= MAX_FAILED_BORROWS {
            // During the mutable borrow, an absurd number of threads have
            // attempted to increment the refcount with immutable borrows.
            // To avoid hypothetically wrapping the refcount, we abort the
            // process once a certain threshold is reached.
            //
            // This requires billions of borrows to fail during the scope of
            // one mutable borrow, and so is very unlikely to happen in a real
            // program.
            //
            // To avoid a potential unsound state after overflowing, we make
            // sure the entire process aborts.
            //
            // Right now, there's no stable way to do that without `std`:
            // https://github.com/rust-lang/rust/issues/67952
            // As a workaround, we cause an abort by making this thread panic
            // during the unwinding of another panic.
            //
            // On platforms where the panic strategy is already 'abort', the
            // ForceAbort object here has no effect, as the program already
            // panics before it is dropped.
            struct ForceAbort;
            impl Drop for ForceAbort {
                fn drop(&mut self) {
                    panic!("Aborting to avoid unsound state of AtomicRefCell");
                }
            }
            let _abort = ForceAbort;
            panic!("Too many failed borrows");
        }
    }

    #[inline]
    fn release(&self, borrow: &AtomicUsize) {
        let old = borrow.fetch_sub(1, atomic::Ordering::Release);
        // This assertion is technically incorrect in the case where another
        // thread hits the hypothetical overflow case, since we might observe
        // the refcount before it fixes it up (and panics). But that never will
        // never happen in a real program, and this is a debug_assert! anyway.
        debug_assert!(old & HIGH_BIT == 0);
    }
}

struct AtomicBorrowRefMut;

impl AtomicBorrowRefMut {
    #[inline]
    fn try_new(borrow: &AtomicUsize) -> Result<Self, &'static str> {
        // Use compare-and-swap to avoid corrupting the immutable borrow count
        // on illegal mutable borrows.
        let old = match borrow.compare_exchange(
            0,
            HIGH_BIT,
            atomic::Ordering::Acquire,
            atomic::Ordering::Relaxed,
        ) {
            Ok(x) => x,
            Err(x) => x,
        };

        if old == 0 {
            Ok(AtomicBorrowRefMut)
        } else if old & HIGH_BIT == 0 {
            Err("already immutably borrowed")
        } else {
            Err("already mutably borrowed")
        }
    }

    #[inline]
    fn release(&self, borrow: &AtomicUsize) {
        borrow.store(0, atomic::Ordering::Release);
    }
}

unsafe impl<T: ?Sized + Send> Send for AtomicRefCell<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for AtomicRefCell<T> {}

//
// End of core synchronization logic. No tricky thread stuff allowed below
// this point.
//

impl<T: Clone> Clone for AtomicRefCell<T> {
    #[inline]
    fn clone(&self) -> AtomicRefCell<T> {
        AtomicRefCell::new((*self.borrow()).clone())
    }
}

impl<T: Default> Default for AtomicRefCell<T> {
    #[inline]
    fn default() -> AtomicRefCell<T> {
        AtomicRefCell::new(Default::default())
    }
}

impl<T: ?Sized + PartialEq> PartialEq for AtomicRefCell<T> {
    #[inline]
    fn eq(&self, other: &AtomicRefCell<T>) -> bool {
        *self.borrow() == *other.borrow()
    }
}

impl<T: ?Sized + Eq> Eq for AtomicRefCell<T> {}

impl<T: ?Sized + PartialOrd> PartialOrd for AtomicRefCell<T> {
    #[inline]
    fn partial_cmp(&self, other: &AtomicRefCell<T>) -> Option<cmp::Ordering> {
        self.borrow().partial_cmp(&*other.borrow())
    }
}

impl<T: ?Sized + Ord> Ord for AtomicRefCell<T> {
    #[inline]
    fn cmp(&self, other: &AtomicRefCell<T>) -> cmp::Ordering {
        self.borrow().cmp(&*other.borrow())
    }
}

impl<T> From<T> for AtomicRefCell<T> {
    fn from(t: T) -> AtomicRefCell<T> {
        AtomicRefCell::new(t)
    }
}

impl<T: ?Sized> Clone for ArcCell<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T: Default> Default for ArcCell<T> {
    #[inline]
    fn default() -> ArcCell<T> {
        ArcCell::new(Default::default())
    }
}

impl<T: ?Sized + PartialEq> PartialEq for ArcCell<T> {
    #[inline]
    fn eq(&self, other: &ArcCell<T>) -> bool {
        *self.borrow() == *other.borrow()
    }
}

impl<T: ?Sized + Eq> Eq for ArcCell<T> {}

impl<T: ?Sized + PartialOrd> PartialOrd for ArcCell<T> {
    #[inline]
    fn partial_cmp(&self, other: &ArcCell<T>) -> Option<cmp::Ordering> {
        self.borrow().partial_cmp(&*other.borrow())
    }
}

impl<T: ?Sized + Ord> Ord for ArcCell<T> {
    #[inline]
    fn cmp(&self, other: &ArcCell<T>) -> cmp::Ordering {
        self.borrow().cmp(&*other.borrow())
    }
}

impl<T> From<T> for ArcCell<T> {
    fn from(t: T) -> ArcCell<T> {
        ArcCell::new(t)
    }
}

struct AtomicRefGuard<'b> {
    count: &'b AtomicUsize,
    borrow: AtomicBorrowRef,
}

impl<'b> Drop for AtomicRefGuard<'b> {
    fn drop(&mut self) {
        self.borrow.release(self.count);
    }
}

impl<'b> Clone for AtomicRefGuard<'b> {
    #[inline]
    #[track_caller]
    fn clone(&self) -> Self {
        Self {
            count: self.count,
            borrow: AtomicBorrowRef::try_new(self.count).unwrap(),
        }
    }
}

/// A wrapper type for an immutably borrowed value from an `AtomicRefCell<T>`.
pub struct AtomicRef<'b, T: ?Sized + 'b> {
    value: &'b T,
    guard: AtomicRefGuard<'b>,

    #[cfg(feature = "track-leases")]
    lease: Lease,
}

impl<'b, T: ?Sized> Deref for AtomicRef<'b, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        self.value
    }
}

impl<'b, T: ?Sized> Clone for AtomicRef<'b, T> {
    #[inline]
    #[track_caller]
    fn clone(&self) -> AtomicRef<'b, T> {
        AtomicRef {
            value: self.value,
            guard: self.guard.clone(),

            #[cfg(feature = "track-leases")]
            lease: self.lease.tracker().lease_at_caller(Some("immutable")),
        }
    }
}

impl<'b, T: ?Sized> AtomicRef<'b, T> {
    /// Make a new `AtomicRef` for a component of the borrowed data.
    #[inline]
    pub fn map<U: ?Sized, F>(orig: AtomicRef<'b, T>, f: F) -> AtomicRef<'b, U>
    where
        F: FnOnce(&T) -> &U,
    {
        AtomicRef {
            value: f(orig.value),
            guard: orig.guard,

            #[cfg(feature = "track-leases")]
            lease: orig.lease,
        }
    }

    /// Make a new `AtomicRef` for an optional component of the borrowed data.
    #[inline]
    pub fn filter_map<U: ?Sized, F>(orig: AtomicRef<'b, T>, f: F) -> Option<AtomicRef<'b, U>>
    where
        F: FnOnce(&T) -> Option<&U>,
    {
        Some(AtomicRef {
            value: f(orig.value)?,
            guard: orig.guard,

            #[cfg(feature = "track-leases")]
            lease: orig.lease,
        })
    }
}

struct AtomicRefMutGuard<'b> {
    count: &'b AtomicUsize,
    borrow: AtomicBorrowRefMut,
}

impl<'b> Drop for AtomicRefMutGuard<'b> {
    fn drop(&mut self) {
        self.borrow.release(self.count);
    }
}

/// A wrapper type for a mutably borrowed value from an `AtomicRefCell<T>`.
pub struct AtomicRefMut<'b, T: ?Sized + 'b> {
    value: &'b mut T,
    guard: AtomicRefMutGuard<'b>,

    #[cfg(feature = "track-leases")]
    lease: Lease,
}

impl<'b, T: ?Sized> AtomicRefMut<'b, T> {
    /// Make a new `AtomicRefMut` for a component of the borrowed data, e.g. an enum
    /// variant.
    #[inline]
    pub fn map<U: ?Sized, F>(orig: AtomicRefMut<'b, T>, f: F) -> AtomicRefMut<'b, U>
    where
        F: FnOnce(&mut T) -> &mut U,
    {
        AtomicRefMut {
            value: f(orig.value),
            guard: orig.guard,

            #[cfg(feature = "track-leases")]
            lease: orig.lease,
        }
    }

    /// Make a new `AtomicRefMut` for an optional component of the borrowed data.
    #[inline]
    pub fn filter_map<U: ?Sized, F>(orig: AtomicRefMut<'b, T>, f: F) -> Option<AtomicRefMut<'b, U>>
    where
        F: FnOnce(&mut T) -> Option<&mut U>,
    {
        Some(AtomicRefMut {
            value: f(orig.value)?,
            guard: orig.guard,

            #[cfg(feature = "track-leases")]
            lease: orig.lease,
        })
    }
}

impl<'b, T: ?Sized> Deref for AtomicRefMut<'b, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        self.value
    }
}

impl<'b, T: ?Sized> DerefMut for AtomicRefMut<'b, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        self.value
    }
}

impl<'b, T: ?Sized + Debug + 'b> Debug for AtomicRef<'b, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.value.fmt(f)
    }
}

impl<'b, T: ?Sized + Debug + 'b> Debug for AtomicRefMut<'b, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.value.fmt(f)
    }
}

struct ArcRefGuard<C: ?Sized> {
    cell: Arc<AtomicRefCell<C>>,
    borrow: AtomicBorrowRef,
}

impl<C: ?Sized> Drop for ArcRefGuard<C> {
    fn drop(&mut self) {
        self.borrow.release(&self.cell.borrows);
    }
}

/// A wrapper type for an immutably borrowed value from an `ArcRefCell<T>`.
pub struct ArcRef<T: ?Sized, C: ?Sized = T> {
    value: *const T,
    guard: ArcRefGuard<C>,

    #[cfg(feature = "track-leases")]
    lease: Lease,
}

unsafe impl<T: Send + Sync, C: ?Sized + Send + Sync> Send for ArcRef<T, C> {}
unsafe impl<T: Send + Sync, C: ?Sized> Sync for ArcRef<T, C> {}

impl<T: ?Sized, C: ?Sized> Deref for ArcRef<T, C> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        unsafe { &*self.value }
    }
}

impl<T: ?Sized, C: ?Sized> Clone for ArcRef<T, C> {
    fn clone(&self) -> Self {
        ArcRef {
            value: self.value,
            guard: ArcRefGuard {
                cell: self.guard.cell.clone(),
                borrow: AtomicBorrowRef::try_new(&self.guard.cell.borrows).unwrap(),
            },

            #[cfg(feature = "track-leases")]
            lease: self.lease.tracker().lease_at_caller(Some("immutable")),
        }
    }
}

impl<T: ?Sized, C: ?Sized> ArcRef<T, C> {
    /// Make a new `ArcRef` for a component of the borrowed data.
    #[inline]
    pub fn map<U: ?Sized, F>(orig: ArcRef<T, C>, f: F) -> ArcRef<U, C>
    where
        F: FnOnce(&T) -> &U,
    {
        ArcRef {
            value: f(unsafe { &*orig.value }),
            guard: orig.guard,

            #[cfg(feature = "track-leases")]
            lease: orig.lease,
        }
    }

    /// Make a new `ArcRef` for an optional component of the borrowed data.
    #[inline]
    pub fn filter_map<U: ?Sized, F>(orig: ArcRef<T, C>, f: F) -> Option<ArcRef<U, C>>
    where
        F: FnOnce(&T) -> Option<&U>,
    {
        Some(ArcRef {
            value: f(unsafe { &*orig.value })?,
            guard: orig.guard,

            #[cfg(feature = "track-leases")]
            lease: orig.lease,
        })
    }
}

struct ArcRefMutGuard<C: ?Sized> {
    cell: Arc<AtomicRefCell<C>>,
    borrow: AtomicBorrowRefMut,
}

impl<C: ?Sized> Drop for ArcRefMutGuard<C> {
    fn drop(&mut self) {
        self.borrow.release(&self.cell.borrows);
    }
}

/// A wrapper type for a mutably borrowed value from an `ArcRefCell<T>`.
pub struct ArcRefMut<T: ?Sized, C: ?Sized = T> {
    value: *mut T,
    guard: ArcRefMutGuard<C>,

    #[cfg(feature = "track-leases")]
    lease: Lease,
}

unsafe impl<T: Send + Sync, C: ?Sized + Send + Sync> Send for ArcRefMut<T, C> {}
unsafe impl<T: Send + Sync, C: ?Sized> Sync for ArcRefMut<T, C> {}

impl<T: ?Sized, C: ?Sized> ArcRefMut<T, C> {
    /// Make a new `ArcRefMut` for a component of the borrowed data.
    #[inline]
    pub fn map<U: ?Sized, F>(orig: ArcRefMut<T, C>, f: F) -> ArcRefMut<U, C>
    where
        F: FnOnce(&mut T) -> &mut U,
    {
        ArcRefMut {
            value: f(unsafe { &mut *orig.value }),
            guard: orig.guard,

            #[cfg(feature = "track-leases")]
            lease: orig.lease,
        }
    }

    /// Make a new `ArcRef` for an optional component of the borrowed data.
    #[inline]
    pub fn filter_map<U: ?Sized, F>(orig: ArcRefMut<T, C>, f: F) -> Option<ArcRefMut<U, C>>
    where
        F: FnOnce(&mut T) -> Option<&mut U>,
    {
        Some(ArcRefMut {
            value: f(unsafe { &mut *orig.value })?,
            guard: orig.guard,

            #[cfg(feature = "track-leases")]
            lease: orig.lease,
        })
    }
}

impl<T: ?Sized, C: ?Sized> Deref for ArcRefMut<T, C> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        unsafe { &*self.value }
    }
}

impl<T: ?Sized, C: ?Sized> DerefMut for ArcRefMut<T, C> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.value }
    }
}

impl<T: ?Sized + Debug, C: ?Sized> Debug for ArcRef<T, C> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.value.fmt(f)
    }
}

impl<T: ?Sized + Debug, C: ?Sized> Debug for ArcRefMut<T, C> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.value.fmt(f)
    }
}

impl<T: ?Sized + Debug> Debug for ArcCell<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ArcCell {{ ... }}")
    }
}

impl<T: ?Sized + Debug> Debug for AtomicRefCell<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "AtomicRefCell {{ ... }}")
    }
}

impl<T: ?Sized> NonBlockingGuardedBorrow<T> for AtomicRefCell<T> {
    type Guard<'a>
    where
        T: 'a,
    = AtomicRef<'a, T>;
    type BorrowError<'a>
    where
        T: 'a,
    = BorrowError;

    fn try_nonblocking_guarded_borrow(&self) -> Result<Self::Guard<'_>, Self::BorrowError<'_>> {
        self.try_borrow()
    }
}

impl<T: ?Sized> NonBlockingGuardedBorrowMut<T> for AtomicRefCell<T> {
    type GuardMut<'a>
    where
        T: 'a,
    = AtomicRefMut<'a, T>;
    type BorrowMutError<'a>
    where
        T: 'a,
    = BorrowMutError;

    fn try_nonblocking_guarded_borrow_mut(
        &self,
    ) -> Result<Self::GuardMut<'_>, Self::BorrowMutError<'_>> {
        self.try_borrow_mut()
    }
}

impl<T: ?Sized> NonBlockingGuardedMutBorrowMut<T> for AtomicRefCell<T> {
    type MutGuardMut<'a>
    where
        T: 'a,
    = &'a mut T;
    type MutBorrowMutError<'a>
    where
        T: 'a,
    = Infallible;

    fn try_nonblocking_guarded_mut_borrow_mut(
        &mut self,
    ) -> Result<Self::MutGuardMut<'_>, Self::MutBorrowMutError<'_>> {
        Ok(self.get_mut())
    }
}

impl<T: ?Sized> NonBlockingGuardedBorrow<T> for ArcCell<T> {
    type Guard<'a>
    where
        T: 'a,
    = AtomicRef<'a, T>;
    type BorrowError<'a>
    where
        T: 'a,
    = BorrowError;

    fn try_nonblocking_guarded_borrow(&self) -> Result<Self::Guard<'_>, Self::BorrowError<'_>> {
        self.as_inner().try_borrow()
    }
}

impl<T: ?Sized> NonBlockingGuardedBorrowMut<T> for ArcCell<T> {
    type GuardMut<'a>
    where
        T: 'a,
    = AtomicRefMut<'a, T>;
    type BorrowMutError<'a>
    where
        T: 'a,
    = BorrowMutError;

    fn try_nonblocking_guarded_borrow_mut(
        &self,
    ) -> Result<Self::GuardMut<'_>, Self::BorrowMutError<'_>> {
        self.as_inner().try_borrow_mut()
    }
}

impl<T: ?Sized> NonBlockingGuardedMutBorrowMut<T> for ArcCell<T> {
    type MutGuardMut<'a>
    where
        T: 'a,
    = AtomicRefMut<'a, T>;
    type MutBorrowMutError<'a>
    where
        T: 'a,
    = BorrowMutError;

    fn try_nonblocking_guarded_mut_borrow_mut(
        &mut self,
    ) -> Result<Self::MutGuardMut<'_>, Self::MutBorrowMutError<'_>> {
        self.as_inner().try_borrow_mut()
    }
}

impl<T: ?Sized, C: ?Sized> NonBlockingGuardedBorrow<T> for ArcRef<T, C> {
    type Guard<'a>
    where
        T: 'a,
        Self: 'a,
    = &'a T;
    type BorrowError<'a>
    where
        T: 'a,
        Self: 'a,
    = Infallible;

    fn try_nonblocking_guarded_borrow(&self) -> Result<Self::Guard<'_>, Self::BorrowError<'_>> {
        Ok(self)
    }
}

impl<T: ?Sized, C: ?Sized> NonBlockingGuardedMutBorrowMut<T> for ArcRef<T, C> {
    type MutGuardMut<'a>
    where
        T: 'a,
        Self: 'a,
    = &'a mut T;
    type MutBorrowMutError<'a>
    where
        T: 'a,
        Self: 'a,
    = &'static str;

    fn try_nonblocking_guarded_mut_borrow_mut(
        &mut self,
    ) -> Result<Self::MutGuardMut<'_>, Self::MutBorrowMutError<'_>> {
        Err("cannot mutably borrow an `ArcRef`")
    }
}

impl<T: ?Sized, C: ?Sized> NonBlockingGuardedBorrow<T> for ArcRefMut<T, C> {
    type Guard<'a>
    where
        T: 'a,
        Self: 'a,
    = &'a T;
    type BorrowError<'a>
    where
        T: 'a,
        Self: 'a,
    = Infallible;

    fn try_nonblocking_guarded_borrow(&self) -> Result<Self::Guard<'_>, Self::BorrowError<'_>> {
        Ok(self)
    }
}

impl<T: ?Sized, C: ?Sized> NonBlockingGuardedMutBorrowMut<T> for ArcRefMut<T, C> {
    type MutGuardMut<'a>
    where
        T: 'a,
        Self: 'a,
    = &'a mut T;
    type MutBorrowMutError<'a>
    where
        T: 'a,
        Self: 'a,
    = Infallible;

    fn try_nonblocking_guarded_mut_borrow_mut(
        &mut self,
    ) -> Result<Self::MutGuardMut<'_>, Self::MutBorrowMutError<'_>> {
        Ok(self)
    }
}
