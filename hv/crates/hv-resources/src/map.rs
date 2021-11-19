use downcast_rs::{impl_downcast, Downcast};
use fxhash::FxHashMap;
use parking_lot::RwLock;
use std::any::TypeId;

use crate::{
    entry::Entry,
    error::{CantGetResource, NoSuchResource},
    refs::{Ref, RefMut},
};

#[cfg(feature = "fetch")]
use crate::fetch::{CantFetch, Fetch};

/// Types that can be stored in [`Resources`], automatically implemented for all applicable.
///
/// [`Resources`]: struct.Resources.html
pub trait Resource: Downcast + 'static {}

impl<T> Resource for T where T: 'static {}

impl_downcast!(Resource);

/// A [`Resource`] container, for storing at most one resource of each specific type.
///
/// Internally, this is a [`FxHashMap`] of [`TypeId`] to [`RwLock`]. None of the methods are
/// blocking, however: accessing a resource in a way that would break borrow rules will
/// return the [`InvalidBorrow`] error instead.
///
/// [`Resource`]: trait.Resource.html
/// [`FxHashMap`]: ../fxhash/type.FxHashMap.html
/// [`TypeId`]: https://doc.rust-lang.org/std/any/struct.TypeId.html
/// [`RwLock`]: ../parking_lot/type.RwLock.html
/// [`InvalidBorrow`]: enum.InvalidBorrow.html
#[derive(Default)]
pub struct Resources {
    resources: FxHashMap<TypeId, RwLock<Box<dyn Resource>>>,
}

fn downcast_resource<T: Resource>(resource: Box<dyn Resource>) -> T {
    *resource
        .downcast::<T>()
        .unwrap_or_else(|_| panic!("downcasting resources should always succeed"))
}

impl Resources {
    /// Creates an empty container. Functionally identical to [`::default()`].
    ///
    /// [`default`]: #method.default
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if a resource of type `T` exists in the container.
    pub fn contains<T: Resource>(&self) -> bool {
        self.resources.contains_key(&TypeId::of::<T>())
    }

    /// Inserts the given resource of type `T` into the container.
    ///
    /// If a resource of this type was already present,
    /// it will be updated, and the original returned.
    pub fn insert<T: Resource>(&mut self, resource: T) -> Option<T> {
        self.resources
            .insert(TypeId::of::<T>(), RwLock::new(Box::new(resource)))
            .map(|resource| downcast_resource(resource.into_inner()))
    }

    /// Removes the resource of type `T` from the container.
    ///
    /// If a resource of this type was present in the container, it will be returned.
    pub fn remove<T: Resource>(&mut self) -> Option<T> {
        self.resources
            .remove(&TypeId::of::<T>())
            .map(|resource| downcast_resource(resource.into_inner()))
    }

    /// Gets the type `T`'s corresponding entry for in-place manipulation.
    pub fn entry<T: Resource>(&mut self) -> Entry<T> {
        Entry::from_hash_map_entry(self.resources.entry(TypeId::of::<T>()))
    }

    /// Returns a reference to the stored resource of type `T`.
    ///
    /// If such a resource is currently accessed mutably elsewhere,
    /// or is not present in the container, returns the appropriate error.
    pub fn get<T: Resource>(&self) -> Result<Ref<T>, CantGetResource> {
        self.resources
            .get(&TypeId::of::<T>())
            .ok_or_else(|| NoSuchResource.into())
            .and_then(|lock| Ref::from_lock(lock).map_err(|error| error.into()))
    }

    /// Returns a mutable reference to the stored resource of type `T`.
    ///
    /// If such a resource is currently accessed immutably or mutably elsewhere,
    /// or is not present in the container, returns the appropriate error.
    pub fn get_mut<T: Resource>(&self) -> Result<RefMut<T>, CantGetResource> {
        self.resources
            .get(&TypeId::of::<T>())
            .ok_or_else(|| NoSuchResource.into())
            .and_then(|lock| RefMut::from_lock(lock).map_err(|error| error.into()))
    }

    /// Retrieves up to 16 resources of any combination of mutability.
    ///
    /// The generic parameter accepts a single one or any tuple (up to 16)
    /// of immutable or mutable references of types that are to be retrieved.
    ///
    /// # Example
    /// ```rust
    /// # use hv_resources::Resources;
    /// let mut resources = Resources::new();
    /// assert!(resources.insert(0f32).is_none());
    /// assert!(resources.insert(1u32).is_none());
    /// assert!(resources.insert(2usize).is_none());
    /// {
    ///     let res_f32 = resources.fetch::<&f32>().unwrap();
    ///     assert_eq!(*res_f32, 0f32);
    /// }
    /// {
    ///     let (mut res_f32, res_u32) = resources.fetch::<(&mut f32, &u32)>().unwrap();
    ///     assert_eq!(*res_u32, 1u32);
    ///     *res_f32 += *res_u32 as f32;
    /// }
    /// {
    ///     let (res_f32, res_usize) = resources.fetch::<(&f32, &usize)>().unwrap();
    ///     assert_eq!(*res_f32, 1f32);
    ///     assert_eq!(*res_usize, 2usize);
    ///     assert!(resources.fetch::<&mut f32>().is_err()); // f32 is already borrowed.
    /// }
    /// assert!(resources.fetch::<&bool>().is_err());// There is no bool in the container.
    /// ```
    #[cfg(feature = "fetch")]
    pub fn fetch<R>(&self) -> Result<<R as Fetch>::Refs, CantFetch>
    where
        for<'a> R: Fetch<'a>,
    {
        R::fetch(self)
    }

    /// View the [`Resources`] with a wrapper that allows for [`Sync`] access.
    pub fn as_sync(&self) -> SyncResources {
        SyncResources { wrapped: self }
    }
}

/// A wrapper over a [`Resources`] which permits only `Send` resources.
///
/// You can convert a regular [`Resources`] into this, but you cannot convert it back!
#[derive(Default)]
pub struct SendResources {
    wrapped: Resources,
}

impl SendResources {
    /// Creates an empty container. Functionally identical to [`::default()`].
    ///
    /// [`default`]: #method.default
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if a resource of type `T` exists in the container.
    pub fn contains<T: Resource + Send>(&self) -> bool {
        self.wrapped.contains::<T>()
    }

    /// Inserts the given resource of type `T` into the container.
    ///
    /// If a resource of this type was already present,
    /// it will be updated, and the original returned.
    pub fn insert<T: Resource + Send>(&mut self, resource: T) -> Option<T> {
        self.wrapped.insert(resource)
    }

    /// Removes the resource of type `T` from the container.
    ///
    /// If a resource of this type was present in the container, it will be returned.
    pub fn remove<T: Resource + Send>(&mut self) -> Option<T> {
        self.wrapped.remove()
    }

    /// Gets the type `T`'s corresponding entry for in-place manipulation.
    pub fn entry<T: Resource + Send>(&mut self) -> Entry<T> {
        self.wrapped.entry()
    }

    /// Returns a reference to the stored resource of type `T`.
    ///
    /// If such a resource is currently accessed mutably elsewhere,
    /// or is not present in the container, returns the appropriate error.
    pub fn get<T: Resource + Send>(&self) -> Result<Ref<T>, CantGetResource> {
        self.wrapped.get()
    }

    /// Returns a mutable reference to the stored resource of type `T`.
    ///
    /// If such a resource is currently accessed immutably or mutably elsewhere,
    /// or is not present in the container, returns the appropriate error.
    pub fn get_mut<T: Resource + Send>(&self) -> Result<RefMut<T>, CantGetResource> {
        self.wrapped.get_mut()
    }

    /// Retrieves up to 16 resources of any combination of mutability.
    ///
    /// The generic parameter accepts a single one or any tuple (up to 16)
    /// of immutable or mutable references of types that are to be retrieved.
    ///
    /// # Example
    /// ```rust
    /// # use hv_resources::SendResources;
    /// let mut resources = SendResources::new();
    /// assert!(resources.insert(0f32).is_none());
    /// assert!(resources.insert(1u32).is_none());
    /// assert!(resources.insert(2usize).is_none());
    /// {
    ///     let res_f32 = resources.fetch::<&f32>().unwrap();
    ///     assert_eq!(*res_f32, 0f32);
    /// }
    /// {
    ///     let (mut res_f32, res_u32) = resources.fetch::<(&mut f32, &u32)>().unwrap();
    ///     assert_eq!(*res_u32, 1u32);
    ///     *res_f32 += *res_u32 as f32;
    /// }
    /// {
    ///     let (res_f32, res_usize) = resources.fetch::<(&f32, &usize)>().unwrap();
    ///     assert_eq!(*res_f32, 1f32);
    ///     assert_eq!(*res_usize, 2usize);
    ///     assert!(resources.fetch::<&mut f32>().is_err()); // f32 is already borrowed.
    /// }
    /// assert!(resources.fetch::<&bool>().is_err());// There is no bool in the container.
    /// ```
    #[cfg(feature = "fetch")]
    pub fn fetch<R>(&self) -> Result<<R as Fetch>::Refs, CantFetch>
    where
        for<'a> R: Fetch<'a>,
        for<'f> <R as Fetch<'f>>::Refs: Send,
    {
        R::fetch(&self.wrapped)
    }

    /// View the [`SendResources`] with a wrapper that allows for [`Sync`] access.
    pub fn as_sync(&self) -> SyncResources {
        SyncResources {
            wrapped: &self.wrapped,
        }
    }
}

/// A wrapper allowing for [`Sync`] usage of a [`Resources`] container. While [`Resources`] is
/// `Send + !Sync`, `SyncResources` is `Send + Sync` - but only allows you to access `Sync` types.
#[derive(Clone, Copy)]
pub struct SyncResources<'a> {
    wrapped: &'a Resources,
}

unsafe impl<'a> Send for SyncResources<'a> {}
unsafe impl<'a> Sync for SyncResources<'a> {}

impl<'a> SyncResources<'a> {
    /// Returns a reference to the stored resource of type `T`.
    ///
    /// If such a resource is currently accessed mutably elsewhere,
    /// or is not present in the container, returns the appropriate error.
    pub fn get<T: Resource + Sync>(&self) -> Result<Ref<T>, CantGetResource> {
        self.wrapped
            .resources
            .get(&TypeId::of::<T>())
            .ok_or_else(|| NoSuchResource.into())
            .and_then(|lock| Ref::from_lock(lock).map_err(|error| error.into()))
    }

    /// Returns a mutable reference to the stored resource of type `T`.
    ///
    /// If such a resource is currently accessed immutably or mutably elsewhere,
    /// or is not present in the container, returns the appropriate error.
    pub fn get_mut<T: Resource + Sync>(&self) -> Result<RefMut<T>, CantGetResource> {
        self.wrapped
            .resources
            .get(&TypeId::of::<T>())
            .ok_or_else(|| NoSuchResource.into())
            .and_then(|lock| RefMut::from_lock(lock).map_err(|error| error.into()))
    }

    /// Retrieves up to 16 resources of any combination of mutability.
    ///
    /// The generic parameter accepts a single one or any tuple (up to 16)
    /// of immutable or mutable references of types that are to be retrieved.
    ///
    /// # Example
    /// ```rust
    /// # use hv_resources::Resources;
    /// let mut resources = Resources::new();
    /// assert!(resources.insert(0f32).is_none());
    /// assert!(resources.insert(1u32).is_none());
    /// assert!(resources.insert(2usize).is_none());
    /// {
    ///     let res_f32 = resources.fetch::<&f32>().unwrap();
    ///     assert_eq!(*res_f32, 0f32);
    /// }
    /// {
    ///     let (mut res_f32, res_u32) = resources.fetch::<(&mut f32, &u32)>().unwrap();
    ///     assert_eq!(*res_u32, 1u32);
    ///     *res_f32 += *res_u32 as f32;
    /// }
    /// {
    ///     let (res_f32, res_usize) = resources.fetch::<(&f32, &usize)>().unwrap();
    ///     assert_eq!(*res_f32, 1f32);
    ///     assert_eq!(*res_usize, 2usize);
    ///     assert!(resources.fetch::<&mut f32>().is_err()); // f32 is already borrowed.
    /// }
    /// assert!(resources.fetch::<&bool>().is_err());// There is no bool in the container.
    /// ```
    #[cfg(feature = "fetch")]
    pub fn fetch<R>(&self) -> Result<<R as Fetch>::Refs, CantFetch>
    where
        for<'f> R: Fetch<'f>,
        for<'f> <R as Fetch<'f>>::Refs: Sync,
    {
        R::fetch(self.wrapped)
    }
}
