//! *Alchemy - 'the process of transmutation by which to fuse or reunite with the divine or original
//! form'.* (Wikipedia)
//!
//! Functionality for dynamically examining and manipulating `dyn Trait` objects. This is done
//! mainly through [`TypeTable`], which is a bit like a superpowered
//! [`TypeId`](core::any::TypeId) that also allows you to ask whether the type it pertains to
//! implements some object safe trait. If you can write it as a `dyn Trait`, then you can try to get
//! a [`DynVtable`] corresponding to that trait's implementation for the type the [`TypeTable`]
//! corresponds to.
//!
//! A few basic relationships:
//! - [`TypeTable`] => one per type, potentially many [`DynVtable`]s per [`TypeTable`]
//!     - A single [`TypeTable`] provides all the information necessary to dynamically allocate,
//!       deallocate, drop, and manipulate the type it corresponds to.
//! - [`Type`] => just a wrapper around [`TypeTable`] that adds the type, for use in
//!   dynamically dispatching over [`Type<T>`]. Useful for when you want to represent
//!   the type of an object in a way that can be `Box<dyn Any>` or `Box<dyn AlchemicalAny>`'d.
//! - [`DynVtable`] => at most one per pair of types (object-safe trait `dyn` type, implementor
//!   type.) Contains the necessary metadata to reconstruct a `*const` or `*mut dyn Trait` for the
//!   object-safe trait type it pertains to, pointing to the implementor type it pertains to.
//!
//! Non-object-safe traits can also be represented using this crate, but they have to be done
//! through blanket-impl'd object-safe traits. For a couple of builtin examples, the [`Clone`] trait
//! is represented through [`AlchemicalClone`], and the [`Copy`] trait is represented through
//! [`AlchemicalCopy`]. Although you cannot directly see a type as `Clone` or `Copy` through its
//! `DynVtable`, you can still do equivalent things (and make use of the consequences of a type
//! being `Clone` or `Copy`.) Also see [`try_clone`](crate::AlchemicalAny::try_clone) and
//! [`try_copy`](crate::AlchemicalAny::try_copy).
//!
//! # Traits
//!
//! Several traits are used to safely represent the transmutations used inside this crate. The two
//! most important ones, which govern whether or not a type can be seen as a `dyn Trait` for some
//! `Trait`, are [`Alchemy`] and [`Alchemical`]. The [`Alchemy`] trait represents a `dyn Trait`
//! type/an object-safe trait, while [`Alchemical<U>`] is blanket-implemented for all types which
//! implement and can be converted to `dyn` objects of some trait `U` (which is a `dyn Trait` type.)
//!
//! There are also a handful of other convenient traits included:
//! - [`AlchemicalAny`] is a powered-up version of [`Any`] which allows for easily fetching the
//!   corresponding [`TypeTable`] for a type.
//! - [`AlchemicalClone`] is an object-safe [`Clone`] abstraction which can allow for cloning boxed
//!   [`dyn AlchemicalAny`](AlchemicalAny) objects.
//! - [`AlchemicalCopy`] is an object-safe [`Copy`] abstraction which can allow for copying
//!   [`AlchemicalAny`] objects.
//!
//! # Caveats
//!
//! In order for an `TypeTable` to be useful with respect to some object-safe trait `U`
//! implemented for some type `T`, that trait's impl for `T` has to be registered with the global
//! static registry, which is initialized at program runtime. There are a number of ways to do this,
//! but the most convenient is [`Type::add`] (and also the related `mark_copy` and
//! `mark_clone`) traits. It's always a good idea to add the copy/clone markings and also `dyn Send`
//! and `dyn Sync` if they can be applied! Note that it is impossible to add a trait which is
//! unimplemented by `T`, so you don't have to worry about causing unsafety or anything with such.
//! This library should (unless some soundness bug has escaped my notice) be completely safe as long
//! as it is kept to its safe API.

#![no_std]
#![feature(ptr_metadata, unsize)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::boxed::Box;
use core::{
    alloc::Layout,
    any::{Any, TypeId},
    fmt,
    hash::Hash,
    marker::{PhantomData, Unsize},
    ptr::{DynMetadata, Pointee},
    sync::atomic::Ordering,
};
use hashbrown::{HashMap, HashSet};
use hv_sync::{atom::AtomSetOnce, cell::AtomicRefCell, monotonic_list::MonotonicList};
use lazy_static::lazy_static;
use spin::RwLock;

/// Convenience function for getting the [`Type`] for some `T`.
pub fn of<T: 'static>() -> Type<T> {
    Type::of()
}

fn typed<T: 'static>() -> Type<T> {
    Type::new()
}

macro_rules! add_types {
    ($m:ident, $closure:expr; $($t:ty),*) => {{
        $({
            let t = <Type<$t>>::new();
            $closure(t);
            $m.insert(TypeId::of::<$t>(), t.as_untyped());
        })*
    }}
}

fn make_registry() -> HashMap<TypeId, &'static TypeTable> {
    fn smart_pointers<T: 'static>(_: Type<T>) {
        use alloc::{
            rc::{Rc, Weak as RcWeak},
            sync::{Arc, Weak as ArcWeak},
        };

        typed::<Rc<T>>().mark_clone();
        typed::<RcWeak<T>>().mark_clone();
        typed::<Arc<T>>().mark_clone();
        typed::<ArcWeak<T>>().mark_clone();
        typed::<&'static T>().mark_clone().mark_copy();
    }

    fn wrappers<T: 'static>(_: Type<T>) {
        smart_pointers::<T>(typed());
        smart_pointers::<core::cell::RefCell<T>>(typed());
        smart_pointers::<AtomicRefCell<T>>(typed());
    }

    fn primitive<T: 'static>(t: Type<T>)
    where
        T: Clone
            + Copy
            + PartialEq
            + Eq
            + PartialOrd
            + Ord
            + Hash
            + fmt::Debug
            + fmt::Display
            + Send
            + Sync,
    {
        t.mark_clone()
            .mark_copy()
            .add::<dyn Send>()
            .add::<dyn Sync>();
        wrappers(t);
    }

    let mut m = HashMap::new();

    // Primitive types
    add_types! {m,
        primitive;

        // unsigned integers
        u8, u16, u32, u64, u128, usize,

        // signed integers
        i8, i16, i32, i64, i128, isize,

        // string slice
        &'static str
    };

    // Stdlib types

    m
}

lazy_static! {
    static ref ALCHEMY_TABLE_REGISTRY: RwLock<HashMap<TypeId, &'static TypeTable>> =
        RwLock::new(make_registry());
    static ref VALID_ALCHEMY_TABLES: RwLock<HashSet<usize>> = RwLock::new(HashSet::new());
}

/// An object-safe clone trait. Useful to have around as a marker for when a type is [`Clone`], and
/// for easily/efficiently performing the clone.
pub trait AlchemicalClone {
    #[doc(hidden)]
    unsafe fn clone_into_ptr(&self, ptr: *mut u8);
}

impl<T: Clone> AlchemicalClone for T {
    unsafe fn clone_into_ptr(&self, ptr: *mut u8) {
        (&mut *ptr.cast::<T>()).clone_from(self);
    }
}

/// An object-safe copy trait. Useful to have around as a marker for when a type is [`Copy`], and
/// for easily/efficiently performing the copy.
pub trait AlchemicalCopy {
    #[doc(hidden)]
    unsafe fn copy_into_ptr(&self, ptr: *mut u8);
}

impl<T: Copy> AlchemicalCopy for T {
    unsafe fn copy_into_ptr(&self, ptr: *mut u8) {
        *ptr.cast::<T>() = *self;
    }
}

static_assertions::assert_obj_safe!(AlchemicalClone, AlchemicalCopy);

/// An auto-implemented marker trait indicating that a type is a subtype of/is convertible to some
/// type `U`. In most cases, this means that `Self` implements some `Trait` such that `U` is `dyn
/// Trait` and a reference to `Self` can be converted to a reference to `dyn Trait`/`U`.
pub trait Alchemical<U: ?Sized + Alchemy>: Any {
    #[doc(hidden)]
    fn cast_ptr(this: *const Self) -> *const U;
    #[doc(hidden)]
    fn cast_mut_ptr(this: *mut Self) -> *mut U;
}

impl<T: ?Sized + Any + Unsize<U>, U: ?Sized + Alchemy> Alchemical<U> for T {
    #[inline]
    fn cast_ptr(this: *const Self) -> *const U {
        this as *const U
    }

    #[inline]
    fn cast_mut_ptr(this: *mut Self) -> *mut U {
        this as *mut U
    }
}

/// An auto-implemented marker trait indicating that a type is a trait object type.
pub trait Alchemy: Any + Pointee<Metadata = DynMetadata<Self>> {}
impl<T> Alchemy for T where T: ?Sized + Any + Pointee<Metadata = DynMetadata<T>> {}

static_assertions::assert_impl_all!(dyn Any: Alchemy);
static_assertions::assert_impl_all!((): AlchemicalClone);

/// A table of information about a type, effectively acting as a superpowered [`TypeId`].
pub struct TypeTable {
    /// The `TypeId` of this table's type.
    pub id: TypeId,
    /// The layout of this table's type, for allocating/deallocating/copying around.
    pub layout: Layout,
    /// A type-erased destructor for this table's type, which drops a value of that type in place.
    pub drop: unsafe fn(*mut u8),
    /// The string name for this table's type, for debug usage.
    pub type_name: &'static str,

    // Private because of interior mutability, so we want to only expose iteration (and insertion)
    // from the outside.
    vtables: MonotonicList<DynVtable>,

    // Always-there vtables, not stored as part of the monotonic list
    pub(crate) alchemical_any: DynVtable,
    pub(crate) alchemical_clone: AtomSetOnce<&'static DynVtable>,
    pub(crate) alchemical_copy: AtomSetOnce<&'static DynVtable>,
    pub(crate) send: AtomSetOnce<&'static DynVtable>,
    pub(crate) sync: AtomSetOnce<&'static DynVtable>,
}

impl TypeTable {
    fn new<T: 'static>() -> &'static Self {
        unsafe fn drop_ptr<T>(x: *mut u8) {
            x.cast::<T>().drop_in_place()
        }

        let this = Self {
            id: TypeId::of::<T>(),
            layout: Layout::new::<T>(),
            drop: drop_ptr::<T>,
            type_name: core::any::type_name::<T>(),
            vtables: MonotonicList::new(),
            alchemical_any: DynVtable::new::<T, dyn AlchemicalAny>(core::ptr::null()),
            alchemical_clone: AtomSetOnce::empty(),
            alchemical_copy: AtomSetOnce::empty(),
            send: AtomSetOnce::empty(),
            sync: AtomSetOnce::empty(),
        };

        Box::leak(Box::new(this))
    }

    /// Get the alchemy table for some type `T`. This function will always return the same
    /// `&'static` for the same type `T`.
    pub fn of<T: 'static>() -> &'static Self {
        ALCHEMY_TABLE_REGISTRY
            .write()
            .entry(TypeId::of::<T>())
            .or_insert_with(|| {
                let eternal = Self::new::<T>();
                VALID_ALCHEMY_TABLES
                    .write()
                    .insert(eternal as *const _ as usize);
                eternal
            })
    }

    /// Check whether the type implements [`Clone`] (through [`AlchemicalClone`]).
    pub fn is_clone(&self) -> bool {
        !self.alchemical_clone.is_none(Ordering::Relaxed)
    }

    /// Check whether the type implements [`Copy`] (through [`AlchemicalCopy`]).
    pub fn is_copy(&self) -> bool {
        !self.alchemical_copy.is_none(Ordering::Relaxed)
    }

    /// Check whether the type implements some object-safe trait representable as `dyn Trait` type
    /// `U`.
    pub fn is<U: ?Sized + Alchemy>(&self) -> bool {
        self.get::<U>().is_some()
    }

    /// Get the [`DynVtable`] corresponding to an object-safe trait `U`, if this type has an
    /// implementation registered.
    pub fn get<U>(&self) -> Option<&DynVtable>
    where
        U: ?Sized + Alchemy,
    {
        let id = TypeId::of::<U>();
        if id == TypeId::of::<dyn AlchemicalAny>() {
            return Some(&self.alchemical_any);
        } else if id == TypeId::of::<dyn AlchemicalClone>() {
            return self.alchemical_clone.get(Ordering::Relaxed);
        } else if id == TypeId::of::<dyn AlchemicalCopy>() {
            return self.alchemical_copy.get(Ordering::Relaxed);
        } else if id == TypeId::of::<dyn Send>() {
            return self.send.get(Ordering::Relaxed);
        } else if id == TypeId::of::<dyn Sync>() {
            return self.sync.get(Ordering::Relaxed);
        }

        self.vtables
            .iter()
            .find(|dv| dv.dyn_type_id == TypeId::of::<U>())
    }

    /// Get the [`DynVtable`] corresponding to an object-safe trait `U`'s implementation for some
    /// type `T` (which is also the type that this `TypeTable` corresponds to), and insert the
    /// implementation into the `TypeTable` if it's not already present.
    ///
    /// Unlike [`TypeTable::get_or_insert_sized`], this function can deal with an unsized `T`,
    /// but needs a pointer to convert in order to extract a vtable.
    ///
    /// Will panic if `T` is not the type this `TypeTable` corresponds to.
    pub fn get_or_insert<T, U>(&self, ptr: *const T) -> &DynVtable
    where
        T: ?Sized + Alchemical<U>,
        U: ?Sized + Alchemy,
    {
        assert_eq!(TypeId::of::<T>(), self.id);
        match self.get::<U>() {
            Some(table) => table,
            None => {
                let id = TypeId::of::<U>();
                let vtable = DynVtable::new::<T, U>(ptr);
                if id == TypeId::of::<dyn AlchemicalClone>() {
                    let eternal = Box::leak(Box::new(vtable));
                    self.alchemical_clone
                        .set_if_none(eternal, Ordering::Relaxed);
                    eternal
                } else if id == TypeId::of::<dyn AlchemicalCopy>() {
                    let eternal = Box::leak(Box::new(vtable));
                    self.alchemical_copy.set_if_none(eternal, Ordering::Relaxed);
                    eternal
                } else if id == TypeId::of::<dyn Send>() {
                    let eternal = Box::leak(Box::new(vtable));
                    self.send.set_if_none(eternal, Ordering::Relaxed);
                    eternal
                } else if id == TypeId::of::<dyn Sync>() {
                    let eternal = Box::leak(Box::new(vtable));
                    self.sync.set_if_none(eternal, Ordering::Relaxed);
                    eternal
                } else {
                    self.vtables.push(vtable)
                }
            }
        }
    }

    /// Get the [`DynVtable`] corresponding to an object-safe trait `U`'s implementation for some
    /// type `T` (where `T` is also the type this `TypeTable` corresponds to.) If `T` is
    /// `?Sized`, you'll need to use [`TypeTable::get_or_insert`] instead.
    ///
    /// Will panic if `T` is not the type for this table.
    pub fn get_or_insert_sized<T, U>(&self) -> &DynVtable
    where
        T: Alchemical<U>,
        U: ?Sized + Alchemy,
    {
        self.get_or_insert::<T, U>(core::ptr::null())
    }

    /// Mark that this table's type (`T`) is [`Clone`].
    ///
    /// Panics if `T` is not the type of this table.
    pub fn mark_clone<T: Clone + 'static>(&self) -> &Self {
        assert_eq!(TypeId::of::<T>(), self.id);
        self.alchemical_clone.set_if_none(
            Box::leak(Box::new(DynVtable::new::<T, dyn AlchemicalClone>(
                core::ptr::null(),
            ))),
            Ordering::Relaxed,
        );
        self
    }

    /// Mark that this table's type (`T`) is [`Copy`].
    ///
    /// Panics if `T` is not the type of this table.
    pub fn mark_copy<T: Copy + 'static>(&self) -> &Self {
        assert_eq!(TypeId::of::<T>(), self.id);
        self.alchemical_copy.set_if_none(
            Box::leak(Box::new(DynVtable::new::<T, dyn AlchemicalCopy>(
                core::ptr::null(),
            ))),
            Ordering::Relaxed,
        );
        self
    }

    /// Register the vtable for some object-safe trait `U`'s implementation for `T`, the type of
    /// this `TypeTable`. If `T` is `?Sized`, use [`TypeTable::add_with`] (which will need a
    /// pointer.)
    ///
    /// Panics if `T` is not the type of this table.
    pub fn add<T, U>(&self) -> &Self
    where
        T: Alchemical<U>,
        U: ?Sized + Alchemy,
    {
        self.get_or_insert::<T, U>(core::ptr::null());
        self
    }

    /// Register the vtable for some object-safe trait `U`'s implementation for `T`, the type of
    /// this `TypeTable`.
    ///
    /// Panics if `T` is not the type of this table.
    pub fn add_with<T, U>(&self, ptr: *const T) -> &Self
    where
        T: ?Sized + Alchemical<U>,
        U: ?Sized + Alchemy,
    {
        self.get_or_insert::<T, U>(ptr);
        self
    }

    /// Convert this `&'static TypeTable` to a raw pointer.
    pub fn to_ptr(&'static self) -> *const TypeTable {
        self
    }

    /// Get an `&'static TypeTable` back from a raw pointer, ensuring it is valid (will return
    /// `None` if the pointer does not correspond to a previously fetched `TypeTable`.)
    pub fn from_ptr(ptr: *const TypeTable) -> Option<&'static TypeTable> {
        VALID_ALCHEMY_TABLES
            .read()
            .contains(&(ptr as usize))
            .then(|| unsafe { &*ptr })
    }
}

/// A statically-typed wrapper around an [`TypeTable`] which corresponds to the parameter type
/// `T`. Most of the time you'll want to use this when interfacing w/ `TypeTable`s, because it
/// lets you call almost every method on [`TypeTable`] without having to specify `T` every time.
pub struct Type<T: ?Sized>(&'static TypeTable, PhantomData<fn(T)>);

impl<T: ?Sized> Clone for Type<T> {
    fn clone(&self) -> Self {
        Self(self.0, self.1)
    }
}

impl<T: ?Sized> Copy for Type<T> {}

impl<T: ?Sized + 'static> Type<T> {
    fn new() -> Self
    where
        T: Sized,
    {
        Self(TypeTable::new::<T>(), PhantomData)
    }

    /// Get the typed alchemy table corresponding to `T`.
    pub fn of() -> Self
    where
        T: Sized,
    {
        Self(TypeTable::of::<T>(), PhantomData)
    }

    /// Get the vtable of `T`'s implementation of some object-safe trait `U`, if it exists in the
    /// registry.
    pub fn get<U>(self) -> Option<&'static DynVtable>
    where
        T: Alchemical<U>,
        U: ?Sized + Alchemy,
    {
        self.0.get::<U>()
    }

    /// Get the vtable of `T`'s implementation of some object-safe trait `U`. Requires a pointer to
    /// convert and extract a vtable from.
    pub fn get_or_insert<U>(self, ptr: *const T) -> &'static DynVtable
    where
        T: Alchemical<U>,
        U: ?Sized + Alchemy,
    {
        self.0.get_or_insert::<T, U>(ptr)
    }

    /// Get the vtable of `T`'s implementation of some object-safe trait `U`. Requires `T: Sized`;
    /// if your `T` is not `Sized`, use [`Type::get_or_insert`] and provide a pointer
    /// to extract from.
    pub fn get_or_insert_sized<U>(self) -> &'static DynVtable
    where
        T: Sized + Alchemical<U>,
        U: ?Sized + Alchemy,
    {
        self.0.get_or_insert_sized::<T, U>()
    }

    /// Register `T` as [`Clone`].
    pub fn mark_clone(self) -> Self
    where
        T: Clone,
    {
        self.0.mark_clone::<T>();
        self
    }

    /// Register `T` as [`Copy`].
    pub fn mark_copy(self) -> Self
    where
        T: Copy,
    {
        self.0.mark_copy::<T>();
        self
    }

    /// Register `T`'s implementation of some object-safe trait `U`, given a pointer to extract
    /// from, and return `Self` for convenience when adding multiple traits.
    pub fn add_with<U>(self, ptr: *const T) -> Self
    where
        T: Alchemical<U>,
        U: ?Sized + Alchemy,
    {
        self.0.add_with::<T, U>(ptr);
        self
    }

    /// Register `T`'s implementation of some object-safe trait `U`, and return `Self` for
    /// convenience when adding multiple traits.
    pub fn add<U>(self) -> Self
    where
        T: Sized + Alchemical<U>,
        U: ?Sized + Alchemy,
    {
        self.0.add::<T, U>();
        self
    }

    /// Get the [`&'static TypeTable`](TypeTable) underlying this `Type<T>`.
    pub fn as_untyped(self) -> &'static TypeTable {
        self.0
    }
}

/// A vtable for some type `T`'s implementation of some object-safe trait `U`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DynVtable {
    obj_type_id: TypeId,
    dyn_type_id: TypeId,
    metadata: *const (),
}

unsafe impl Send for DynVtable {}
unsafe impl Sync for DynVtable {}

impl DynVtable {
    /// Extract a `DynVtable` from a pointer. No need for the pointer to be valid here; this
    /// function doesn't end up dereferencing the pointer, so if it's just a null pointer, that's
    /// fine. Note that [`std::ptr::null`] does require `T` to be sized, hence why this method takes
    /// a pointer instead of just constructing a null pointer internally.
    pub fn new<T, U>(ptr: *const T) -> Self
    where
        T: ?Sized + Alchemical<U>,
        U: ?Sized + Alchemy,
    {
        let cast_ptr = <T as Alchemical<U>>::cast_ptr(ptr);
        Self {
            obj_type_id: TypeId::of::<T>(),
            dyn_type_id: TypeId::of::<U>(),
            metadata: unsafe {
                let u_metadata = core::ptr::metadata::<U>(cast_ptr);
                core::mem::transmute::<DynMetadata<U>, *const ()>(u_metadata)
            },
        }
    }

    /// Construct a `DynVtable` from its type-erased components.
    ///
    /// # Safety
    ///
    /// `metadata` must be the transmutation or bit-equivalent represeentation of a
    /// [`DynMetadata<U>`] for some `T` where `T: ?Sized + Alchemical<U>` and `U: ?Sized +
    /// Alchemy<T>`, and where `TypeId::of::<T>() == obj_type_id && TypeId::of::<U>() ==
    /// dyn_type_id`.
    pub unsafe fn new_from_parts(
        obj_type_id: TypeId,
        dyn_type_id: TypeId,
        metadata: *const (),
    ) -> Self {
        Self {
            obj_type_id,
            dyn_type_id,
            metadata,
        }
    }

    /// The [`TypeId`] of the type `T` which implements the trait object type `U`.
    pub fn obj_type_id(&self) -> TypeId {
        self.obj_type_id
    }

    /// The [`TypeId`] of the trait object type `U` implemented by the type `T`.
    pub fn dyn_type_id(&self) -> TypeId {
        self.dyn_type_id
    }

    /// Construct a `*const` pointer to the `dyn Trait` object corresponding to this vtable, from an
    /// untyped pointer to a value of the same type as this vtable was created from.
    ///
    /// # Safety
    ///
    /// `object` must be a valid pointer to a value which is of some type `T` where
    /// `TypeId::of::<T>() == self.obj_type_id()`, the type `T` implements `Alchemical<U>` for some
    /// type `U` where `TypeId::of::<U>() == self.dyn_type_id()`, and `U` implements `Alchemy<T>`.
    pub unsafe fn to_dyn_object_ptr<U: Any + ?Sized + Pointee<Metadata = DynMetadata<U>>>(
        &self,
        object: *const (),
    ) -> *const U {
        core::ptr::from_raw_parts::<U>(
            object,
            core::mem::transmute::<*const (), DynMetadata<U>>(self.metadata),
        )
    }

    /// Construct a `*mut` pointer to the `dyn Trait` object corresponding to this vtable, from an
    /// untyped pointer to a value of the same type as this vtable was created from.
    ///
    /// # Safety
    ///
    /// `object` must be a valid pointer to a value which is of some type `T` where
    /// `TypeId::of::<T>() == self.obj_type_id()`, the type `T` implements `Alchemical<U>` for some
    /// type `U` where `TypeId::of::<U>() == self.dyn_type_id()`, and `U` implements `Alchemy<T>`.
    pub unsafe fn to_dyn_object_mut_ptr<U: Any + ?Sized + Pointee<Metadata = DynMetadata<U>>>(
        &self,
        object: *mut (),
    ) -> *mut U {
        core::ptr::from_raw_parts_mut::<U>(
            object,
            core::mem::transmute::<*const (), DynMetadata<U>>(self.metadata),
        )
    }
}

/// A type-erased pointer which knows about some set of vtables belonging to the type of the object
/// it points to.
#[derive(Clone, Copy)]
pub struct AlchemicalPtr {
    data: *mut (),
    table: &'static TypeTable,
}

impl AlchemicalPtr {
    /// Create a new `AlchemicalPtr` from a statically-typed pointer.
    pub fn new<T: Any>(ptr: *mut T) -> Self {
        Self {
            data: ptr.cast(),
            table: TypeTable::of::<T>(),
        }
    }

    /// Get the data pointer of this `AlchemicalPtr`.
    pub fn as_ptr(self) -> *mut () {
        self.data
    }

    /// Get the [`TypeTable`] of the type pointed to by this `AlchemicalPtr`.
    pub fn table(self) -> &'static TypeTable {
        self.table
    }

    /// Equivalent to `AlchemicalPtr::downcast_dyn_ptr::<dyn AlchemicalAny>`, but faster.
    ///
    /// # Safety
    ///
    /// Same safety considerations as [`AlchemicalPtr::downcast_dyn_ptr`].
    pub unsafe fn as_alchemical_any(self) -> *const dyn AlchemicalAny {
        self.table.alchemical_any.to_dyn_object_ptr(self.data)
    }

    /// Equivalent to `AlchemicalPtr::downcast_dyn_mut_ptr::<dyn AlchemicalAny>`, but faster.
    ///
    /// # Safety
    ///
    /// Same safety considerations as [`AlchemicalPtr::downcast_dyn_mut_ptr`].
    pub unsafe fn as_alchemical_any_mut(self) -> *mut dyn AlchemicalAny {
        self.table.alchemical_any.to_dyn_object_mut_ptr(self.data)
    }

    /// Reconstruct an `AlchemicalNonNull` from its raw parts.
    ///
    /// # Safety
    ///
    /// `data` must be a valid pointer, and `table`  must be the `TypeTable` corresponding to the
    /// actual un-erased type that `data` points to.
    pub unsafe fn from_raw_parts(data: *mut (), table: &'static TypeTable) -> Self {
        Self { data, table }
    }

    /// If the vtable corresponding to the type `U` is found in this pointer's `TypeTable`,
    /// construct a "fat" `*const` pointer from it and the data pointer of this `AlchemicalNonNull`.
    pub fn downcast_dyn_ptr<U: ?Sized + Alchemy>(self) -> Option<*const U> {
        self.table
            .get::<U>()
            .map(|vtable| unsafe { vtable.to_dyn_object_ptr::<U>(self.data.cast()) })
    }

    /// If the vtable corresponding to the type `U` is found in this pointer's `TypeTable`,
    /// construct a "fat" `*mut` pointer from it and the data pointer of this `AlchemicalNonNull`.
    pub fn downcast_dyn_mut_ptr<U: ?Sized + Alchemy>(self) -> Option<*mut U> {
        self.table
            .get::<U>()
            .map(|vtable| unsafe { vtable.to_dyn_object_mut_ptr::<U>(self.data.cast()) })
    }

    /// Downcast the pointer to an immutable reference to a given trait object, if it implements it.
    ///
    /// # Safety
    ///
    /// This `AlchemicalNonNull` must point to a valid object of the type registered in its internal
    /// `TypeTable`. In addition, it must be safe to immutably borrow that object. This function
    /// also returns a completely arbitrary lifetime, so be sure that it does not outlive the
    /// pointee.
    pub unsafe fn downcast_dyn_ref<'a, U: ?Sized + Alchemy>(self) -> Option<&'a U> {
        self.table
            .get::<U>()
            .map(|vtable| &*unsafe { vtable.to_dyn_object_ptr::<U>(self.data.cast()) })
    }

    /// Downcast the pointer to a mutable reference to a given trait object, if it implements it.
    ///
    /// # Safety
    ///
    /// This `AlchemicalNonNull` must point to a valid object of the type registered in its internal
    /// `TypeTable`. In addition, it must be safe to mutably borrow that object. This function
    /// also returns a completely arbitrary lifetime, so be sure that it does not outlive the
    /// pointee.
    pub unsafe fn downcast_dyn_mut<'a, U: ?Sized + Alchemy>(self) -> Option<&'a mut U> {
        self.table
            .get::<U>()
            .map(|vtable| &mut *unsafe { vtable.to_dyn_object_mut_ptr::<U>(self.data.cast()) })
    }
}

/// A superpowered version of [`Any`] which provides a [`TypeTable`] rather than a [`TypeId`].
pub trait AlchemicalAny {
    /// Get the alchemy table of the underlying type.
    fn type_table(&self) -> &'static TypeTable;
}

impl<T: Any> AlchemicalAny for T {
    fn type_table(&self) -> &'static TypeTable {
        TypeTable::of::<T>()
    }
}

impl dyn AlchemicalAny {
    /// Try to cast this `&dyn AlchemicalAny` to some other trait object `U`.
    pub fn dyncast_ref<U: Alchemy + ?Sized>(&self) -> Option<&U> {
        let at = Self::type_table(self);
        let downcast_alchemy = at.get::<U>()?;
        unsafe {
            Some(
                &*downcast_alchemy
                    .to_dyn_object_ptr::<U>((self as *const dyn AlchemicalAny).cast()),
            )
        }
    }

    /// Try to cast this `&mut dyn AlchemicalAny` to some other trait object `U`.
    pub fn dyncast_mut<U: Alchemy + ?Sized>(&mut self) -> Option<&mut U> {
        let at = Self::type_table(self);
        let downcast_alchemy = at.get::<U>()?;
        unsafe {
            Some(
                &mut *downcast_alchemy
                    .to_dyn_object_mut_ptr::<U>((self as *mut dyn AlchemicalAny).cast()),
            )
        }
    }

    /// Try to cast this `Box<dyn AlchemicalAny>` to some other trait object `U`.
    pub fn dyncast<U: Alchemy + ?Sized>(self: Box<Self>) -> Option<Box<U>> {
        let at = Self::type_table(&self);
        let downcast_alchemy = at.get::<U>()?;
        unsafe {
            let ptr = Box::into_raw(self);
            Some(Box::from_raw(
                downcast_alchemy.to_dyn_object_mut_ptr::<U>(ptr as *mut _),
            ))
        }
    }

    /// Try to cast this `&dyn AlchemicalAny` to some type `T`.
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        let at = Self::type_table(self);
        (at.id == TypeId::of::<T>())
            .then(|| unsafe { &*(self as *const dyn AlchemicalAny as *const T) })
    }

    /// Try to cast this `&mut dyn AlchemicalAny` to some type `T`.
    pub fn downcast_mut<T: Any>(&mut self) -> Option<&mut T> {
        let at = Self::type_table(self);
        (at.id == TypeId::of::<T>())
            .then(|| unsafe { &mut *(self as *mut dyn AlchemicalAny as *mut T) })
    }

    /// Try to cast this `Box<dyn AlchemicalAny>` to some type `T`.
    pub fn downcast<T: Any>(self: Box<Self>) -> Option<Box<T>> {
        let at = Self::type_table(&self);
        (at.id == TypeId::of::<T>())
            .then(|| unsafe { Box::from_raw(Box::into_raw(self) as *mut T) })
    }

    /// Try to copy this value into a `Box<dyn AlchemicalAny>`. If it succeeds, a copy is created
    /// and the original type is not moved (because it implements [`Copy`].)
    pub fn try_copy(&self) -> Option<Box<dyn AlchemicalAny>> {
        let at = self.type_table();
        let as_alchemical_copy = at.get::<dyn AlchemicalCopy>()?;
        unsafe {
            let ptr = alloc::alloc::alloc(at.layout);
            (*as_alchemical_copy.to_dyn_object_ptr::<dyn AlchemicalCopy>(
                (self as *const dyn AlchemicalAny).cast(),
            ))
            .copy_into_ptr(ptr);
            let recast_ptr = at
                .alchemical_any
                .to_dyn_object_mut_ptr::<dyn AlchemicalAny>(ptr as *mut _);
            Some(Box::from_raw(recast_ptr))
        }
    }

    /// Try to clone this value into a `Box<dyn AlchemicalAny>`. If it succeeds, a clone is created
    /// and the original type is not moved (because it implements [`Clone`].)
    pub fn try_clone(&self) -> Option<Box<dyn AlchemicalAny>> {
        let at = self.type_table();
        let as_alchemical_clone = at.get::<dyn AlchemicalClone>()?;
        unsafe {
            let ptr = alloc::alloc::alloc(at.layout);
            (*as_alchemical_clone.to_dyn_object_ptr::<dyn AlchemicalClone>(
                (self as *const dyn AlchemicalAny).cast(),
            ))
            .clone_into_ptr(ptr);
            let recast_ptr = at
                .alchemical_any
                .to_dyn_object_mut_ptr::<dyn AlchemicalAny>(ptr as *mut _);
            Some(Box::from_raw(recast_ptr))
        }
    }
}

/// Returns true if the values were moved.
///
/// # Safety
///
/// Pointers must be valid and DST should be uninitialized (it will *not* be dropped if it is
/// already initialized and will be treated as uninitialized.)
pub unsafe fn clone_or_move(src: *mut dyn AlchemicalAny, dst: *mut u8) -> bool {
    let table = <dyn AlchemicalAny>::type_table(&*src);
    if let Some(clone_vt) = table.get::<dyn AlchemicalClone>() {
        clone_vt
            .to_dyn_object_ptr::<dyn AlchemicalClone>(src as *mut ())
            .clone_into_ptr(dst as *mut u8);
        false
    } else {
        core::ptr::copy_nonoverlapping(src as *const u8, dst as *mut u8, table.layout.size());
        true
    }
}

/// Returns true if the values were moved.
///
/// # Safety
///
/// Pointers must be valid and DST should be uninitialized (it will *not* be dropped if it is
/// already initialized and will be treated as uninitialized.)
pub unsafe fn copy_clone_or_move_to(
    src: *mut dyn AlchemicalAny,
    dst: *mut dyn AlchemicalAny,
) -> bool {
    let table = <dyn AlchemicalAny>::type_table(&*src);
    assert!(core::ptr::eq(table, <dyn AlchemicalAny>::type_table(&*dst)));
    if let Some(copy_vt) = table.get::<dyn AlchemicalCopy>() {
        copy_vt
            .to_dyn_object_ptr::<dyn AlchemicalCopy>(src as *mut ())
            .copy_into_ptr(dst as *mut u8);
        false
    } else if let Some(clone_vt) = table.get::<dyn AlchemicalClone>() {
        clone_vt
            .to_dyn_object_ptr::<dyn AlchemicalClone>(src as *mut ())
            .clone_into_ptr(dst as *mut u8);
        false
    } else {
        core::ptr::copy_nonoverlapping(src as *const u8, dst as *mut u8, table.layout.size());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alchemical_clone() {
        // `mark_clone` is shorthand for `.register_sized::<i32, dyn AlchemicalClone>` (where
        // `AlchemicalClone` is an object-safe but very unsafe trait implementing `Clone`-ing into a
        // pointer) and where `register_sized` is shorthand for `.register::<i32, dyn
        // AlchemicalClone>(core::ptr::null())`
        Type::<i32>::of().mark_clone();

        let boxed: Box<dyn AlchemicalAny> = Box::new(5i32);
        let other: Box<dyn AlchemicalAny> = boxed.try_clone().unwrap();

        let a = *boxed.downcast_ref::<i32>().unwrap();
        let b = *other.downcast_ref::<i32>().unwrap();

        assert_eq!(a, b);
    }

    #[test]
    fn alchemical_copy() {
        // `mark_clone` is shorthand for `.register_sized::<i32, dyn AlchemicalClone>` (where
        // `AlchemicalClone` is an object-safe but very unsafe trait implementing `Clone`-ing into a
        // pointer) and where `register_sized` is shorthand for `.register::<i32, dyn
        // AlchemicalClone>(core::ptr::null())`
        Type::<i32>::of().mark_copy();

        let boxed: Box<dyn AlchemicalAny> = Box::new(5i32);
        let other: Box<dyn AlchemicalAny> = boxed.try_copy().unwrap();

        let a = *boxed.downcast_ref::<i32>().unwrap();
        let b = *other.downcast_ref::<i32>().unwrap();

        assert_eq!(a, b);
    }
}
