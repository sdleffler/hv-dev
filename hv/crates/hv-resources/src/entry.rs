use parking_lot::RwLock;
use std::{any::TypeId, collections::hash_map, marker::PhantomData, ops::DerefMut};

use crate::{
    map::Resource,
    refs::{Ref, RefMut},
};

/// A view into an entry in a [`Resources`] container, which may either be vacant or occupied.
/// This is returned by the [`entry`] method on [`Resources`].
///
/// [`Resources`]: struct.Resources.html
/// [`entry`]: struct.Resources.html#method.entry
pub enum Entry<'a, T: Resource> {
    /// An occupied entry.
    Occupied(OccupiedEntry<'a, T>),
    /// A vacant entry.
    Vacant(VacantEntry<'a, T>),
}

/// A view into an occupied entry in a [`Resources`] container. It is part of the [`Entry`] enum.
///
/// [`Resources`]: struct.Resources.html
/// [`Entry`]: enum.Entry.html
pub struct OccupiedEntry<'a, T: Resource> {
    base: hash_map::OccupiedEntry<'a, TypeId, RwLock<Box<dyn Resource>>>,
    phantom_data: PhantomData<T>,
}

/// A view into a vacant entry in a [`Resources`] container. It is part of the [`Entry`] enum.
///
/// [`Resources`]: struct.Resources.html
/// [`Entry`]: enum.Entry.html
pub struct VacantEntry<'a, T: Resource> {
    base: hash_map::VacantEntry<'a, TypeId, RwLock<Box<dyn Resource>>>,
    phantom_data: PhantomData<T>,
}

impl<'a, T: Resource> Entry<'a, T> {
    pub(crate) fn from_hash_map_entry(
        entry: hash_map::Entry<'a, TypeId, RwLock<Box<dyn Resource>>>,
    ) -> Self {
        match entry {
            hash_map::Entry::Occupied(base) => Entry::Occupied(OccupiedEntry {
                base,
                phantom_data: PhantomData,
            }),
            hash_map::Entry::Vacant(base) => Entry::Vacant(VacantEntry {
                base,
                phantom_data: PhantomData,
            }),
        }
    }

    /// Ensures a resource is in the entry by inserting the given value if empty,
    /// and returns a mutable reference to the contained resource.
    pub fn or_insert(self, default: T) -> RefMut<'a, T> {
        self.or_insert_with(|| default)
    }

    /// Ensures a resource is in the entry by inserting the result of given function if empty,
    /// and returns a mutable reference to the contained resource.
    pub fn or_insert_with(self, default: impl FnOnce() -> T) -> RefMut<'a, T> {
        use Entry::*;
        match self {
            Occupied(occupied) => occupied.into_mut(),
            Vacant(vacant) => vacant.insert(default()),
        }
    }

    /// Provides in-place mutable access to an occupied entry before any potential inserts.
    pub fn and_modify(mut self, f: impl FnOnce(&mut T)) -> Self {
        if let Entry::Occupied(occupied) = &mut self {
            f(occupied.get_mut().deref_mut());
        }
        self
    }
}

impl<'a, T: Resource + Default> Entry<'a, T> {
    /// Ensures a resource is in the entry by inserting it's default value if empty,
    /// and returns a mutable reference to the contained resource.
    pub fn or_default(self) -> RefMut<'a, T> {
        self.or_insert_with(T::default)
    }
}

impl<'a, T: Resource> OccupiedEntry<'a, T> {
    /// Gets a reference to the value in the entry.
    pub fn get(&self) -> Ref<T> {
        Ref::from_lock(self.base.get()).expect("entry API assumes unique access")
    }

    /// Gets a mutable reference to the value in the entry.
    pub fn get_mut(&mut self) -> RefMut<T> {
        RefMut::from_lock(self.base.get_mut()).expect("entry API assumes unique access")
    }

    /// Converts the `OccupiedEntry` into a mutable reference to the value in the entry
    /// with a lifetime bound to the [`Resources`] struct itself.
    ///
    /// [`Resources`]: struct.Resources.html
    pub fn into_mut(self) -> RefMut<'a, T> {
        RefMut::from_lock(self.base.into_mut()).expect("entry API assumes unique access")
    }

    /// Sets the value of the entry, and returns the entry's old value.
    pub fn insert(&mut self, value: T) -> T {
        *self
            .base
            .insert(RwLock::new(Box::new(value)))
            .into_inner()
            .downcast()
            .unwrap_or_else(|_| panic!("downcasting resources should always succeed"))
    }

    /// Takes the value out of the entry, and returns it.
    pub fn remove(self) -> T {
        *self
            .base
            .remove()
            .into_inner()
            .downcast()
            .unwrap_or_else(|_| panic!("downcasting resources should always succeed"))
    }
}

impl<'a, T: Resource> VacantEntry<'a, T> {
    /// Sets the value of the entry, and returns a mutable reference to it.
    pub fn insert(self, value: T) -> RefMut<'a, T> {
        RefMut::from_lock(self.base.insert(RwLock::new(Box::new(value))))
            .expect("entry API assumes unique access")
    }
}
