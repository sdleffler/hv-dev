use std::{ptr::NonNull, thread::panicking};

use super::AtomicBorrow;

/// A pointer to a resource, with runtime borrow checking via an `AtomicBorrow`,
/// accessed through a pointer to a cached one in an executor.
pub struct ResourceCell<R0> {
    cell: NonNull<R0>,
    borrow: NonNull<AtomicBorrow>,
    mutable: bool,
}

impl<R0> ResourceCell<R0> {
    pub fn new_mut(resource: &mut R0, borrow: &mut AtomicBorrow) -> Self {
        Self {
            cell: NonNull::from(resource),
            borrow: NonNull::from(borrow),
            mutable: true,
        }
    }

    pub fn new_shared(resource: &R0, borrow: &mut AtomicBorrow) -> Self {
        let this = Self {
            cell: NonNull::from(resource),
            borrow: NonNull::from(borrow),
            mutable: false,
        };
        // make a "virtual" immutable borrow to prevent any mutable borrows of this resource.
        this.borrow();
        this
    }

    pub fn borrow(&self) -> &R0 {
        assert!(
            unsafe { self.borrow.as_ref().borrow() },
            "cannot borrow {} immutably: already borrowed mutably",
            std::any::type_name::<R0>()
        );
        unsafe { self.cell.as_ref() }
    }

    #[allow(clippy::mut_from_ref)]
    pub fn borrow_mut(&self) -> &mut R0 {
        assert!(
            unsafe { self.borrow.as_ref().borrow_mut() },
            "cannot borrow {} mutably: already borrowed",
            std::any::type_name::<R0>()
        );
        unsafe { &mut *self.cell.as_ptr() }
    }

    pub unsafe fn release(&self) {
        self.borrow.as_ref().release();
    }

    pub unsafe fn release_mut(&self) {
        self.borrow.as_ref().release_mut();
    }
}

impl<R0> Drop for ResourceCell<R0> {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        if !panicking() {
            // if this is not a mutable borrow, we release the "virtual" immutable borrow.
            if !self.mutable {
                unsafe { self.release() };
            }

            assert!(
                unsafe { self.borrow.as_ref().is_free() },
                "borrows of {} were not released properly",
                std::any::type_name::<R0>()
            )
        }
    }
}

unsafe impl<R0> Send for ResourceCell<R0> where R0: Send {}

unsafe impl<R0> Sync for ResourceCell<R0> where R0: Sync {}
