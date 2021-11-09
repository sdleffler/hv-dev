use parking_lot::{
    MappedRwLockReadGuard, MappedRwLockWriteGuard, RwLock, RwLockReadGuard, RwLockWriteGuard,
};
use std::ops::{Deref, DerefMut};

use crate::{InvalidBorrow, Resource};

/// Immutable borrow of a [`Resource`] stored in a [`Resources`] container.
///
/// [`Resource`]: trait.Resource.html
/// [`Resources`]: struct.Resources.html
pub struct Ref<'a, T: Resource> {
    read_guard: MappedRwLockReadGuard<'a, T>,
}

impl<'a, T: Resource> Ref<'a, T> {
    pub(crate) fn from_lock(lock: &'a RwLock<Box<dyn Resource>>) -> Result<Self, InvalidBorrow> {
        lock.try_read()
            .map(|guard| Self {
                read_guard: RwLockReadGuard::map(guard, |resource| {
                    resource
                        .downcast_ref::<T>()
                        .unwrap_or_else(|| panic!("downcasting resources should always succeed"))
                }),
            })
            .ok_or(InvalidBorrow::Immutable)
    }
}

impl<'a, T: Resource> Deref for Ref<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.read_guard.deref()
    }
}

/// Mutable borrow of a [`Resource`] stored in a [`Resources`] container.
///
/// [`Resource`]: trait.Resource.html
/// [`Resources`]: struct.Resources.html
pub struct RefMut<'a, T: Resource> {
    write_guard: MappedRwLockWriteGuard<'a, T>,
}

impl<'a, T: Resource> RefMut<'a, T> {
    pub(crate) fn from_lock(lock: &'a RwLock<Box<dyn Resource>>) -> Result<Self, InvalidBorrow> {
        lock.try_write()
            .map(|guard| Self {
                write_guard: RwLockWriteGuard::map(guard, |resource| {
                    resource
                        .downcast_mut::<T>()
                        .unwrap_or_else(|| panic!("downcasting resources should always succeed"))
                }),
            })
            .ok_or(InvalidBorrow::Mutable)
    }
}

impl<'a, T: Resource> Deref for RefMut<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.write_guard.deref()
    }
}

impl<'a, T: Resource> DerefMut for RefMut<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.write_guard.deref_mut()
    }
}
