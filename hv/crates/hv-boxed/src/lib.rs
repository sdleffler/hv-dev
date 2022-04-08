//! The "`MiniBox`", a solution for holding small trait objects on the stack.
//!
//! Heavily drawn from the `smallbox` crate.

/*

While this is not a direct copy of the `smallbox` crate, it is heavily based on the
smallbox source code, which is licensed MIT as follows:

The MIT License (MIT)

Copyright (c) 2015 John Hodge

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

*/

#![no_std]
#![feature(ptr_metadata)]

use core::{
    any::Any,
    cmp::Ordering,
    fmt,
    hash::{self, Hash},
    marker::PhantomData,
    mem::{self, ManuallyDrop},
    ops::{Deref, DerefMut},
    ptr::{self, Pointee},
};

/// Create a potentially unsized [`MiniBox`].
#[macro_export]
macro_rules! minibox {
    ( $e: expr ) => {{
        let val = $e;
        let ptr = &val as *const _;
        #[allow(unsafe_code)]
        unsafe {
            $crate::MiniBox::new_unchecked(val, ptr)
        }
    }};
}

/// Type alias for an [`MiniBox`] with copyable space.
pub type CopyBox<T, Space> = MiniBox<T, Copyable<T, Space>>;

/// Trait describing how to store a value inside some given storage space. Highly unsafe and you
/// should not need to implement this yourself.
pub trait Storable<T: ?Sized, U: ?Sized>: Storage<T> {
    /// Construct this storage space from a value and a potentially unsize-coerced pointer to the
    /// same value. The pointer is used to carry "fat pointer" metadata.
    #[allow(clippy::missing_safety_doc)]
    unsafe fn new_copy(val: &U, ptr: *const T) -> Self;

    /// The resulting downcasted type.
    type Downcast: Storage<U>;

    /// Downcast an owned space to a different value without checking to ensure the types match up.
    #[allow(clippy::missing_safety_doc)]
    unsafe fn downcast_unchecked(self) -> Self::Downcast
    where
        U: Sized;
}

/// Trait describing storage for a given type.
pub trait Storage<T: ?Sized> {
    /// Get an immutable pointer to a `T` stored inside this storage.
    fn as_ptr(&self) -> *const T;
    /// Get a mutable pointer to a `T` stored inside this storage.
    fn as_mut_ptr(&mut self) -> *mut T;
}

/// Non-copyable storage for an [`MiniBox`].
pub struct NonCopyable<T: ?Sized, Space> {
    metadata: <T as Pointee>::Metadata,
    space: ManuallyDrop<Space>,
    _phantom: PhantomData<T>,
}

impl<T: ?Sized, Space, U: ?Sized> Storable<T, U> for NonCopyable<T, Space> {
    unsafe fn new_copy(val: &U, ptr: *const T) -> Self {
        let size = mem::size_of_val::<U>(val);
        let align = mem::align_of_val::<U>(val);
        let metadata = ptr::metadata(ptr);

        let mut space = mem::MaybeUninit::<Space>::uninit();

        let ptr_copy: *mut u8 = if size == 0 {
            align as *mut u8
        } else if size > mem::size_of::<Space>() || align > mem::align_of::<Space>() {
            panic!("value does not fit inside provided space/with required alignment.");
        } else {
            space.as_mut_ptr() as *mut u8
        };

        ptr::copy_nonoverlapping(val as *const _ as *const u8, ptr_copy, size);

        Self {
            metadata,
            space: ManuallyDrop::new(space.assume_init()),
            _phantom: PhantomData,
        }
    }

    type Downcast = NonCopyable<U, Space>;
    unsafe fn downcast_unchecked(self) -> Self::Downcast
    where
        U: Sized,
    {
        let size = mem::size_of::<U>();
        let mut space = mem::MaybeUninit::<Space>::uninit();

        ptr::copy_nonoverlapping(
            &*self.space as *const _ as *const u8,
            space.as_mut_ptr() as *mut u8,
            size,
        );

        let metadata = ptr::metadata(space.as_ptr() as *const U);

        mem::forget(self);

        NonCopyable {
            metadata,
            space: ManuallyDrop::new(space.assume_init()),
            _phantom: Default::default(),
        }
    }
}

impl<T: ?Sized, Space> Storage<T> for NonCopyable<T, Space> {
    fn as_ptr(&self) -> *const T {
        ptr::from_raw_parts(&self.space as *const _ as *const (), self.metadata)
    }

    fn as_mut_ptr(&mut self) -> *mut T {
        ptr::from_raw_parts_mut(&mut self.space as *mut _ as *mut (), self.metadata)
    }
}

impl<T: ?Sized, Space> Drop for NonCopyable<T, Space> {
    fn drop(&mut self) {
        unsafe {
            ptr::drop_in_place(self.as_mut_ptr());
        }
    }
}

/// Copyable storage for an [`MiniBox`] which only allows copy types to be stored in it.
pub struct Copyable<T: ?Sized, Space: Copy> {
    metadata: <T as Pointee>::Metadata,
    space: ManuallyDrop<Space>,
    _phantom: PhantomData<T>,
}

impl<T: ?Sized, Space: Copy> Clone for Copyable<T, Space> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized, Space: Copy> Copy for Copyable<T, Space> {}

impl<T: ?Sized, Space: Copy, U: Copy> Storable<T, U> for Copyable<T, Space> {
    unsafe fn new_copy(val: &U, ptr: *const T) -> Self {
        let size = mem::size_of_val::<U>(val);
        let align = mem::align_of_val::<U>(val);
        let metadata = ptr::metadata(ptr);

        let mut space = mem::MaybeUninit::<Space>::uninit();

        let ptr_copy: *mut u8 = if size == 0 {
            align as *mut u8
        } else if size > mem::size_of::<Space>() || align > mem::align_of::<Space>() {
            panic!("value does not fit inside provided space/with required alignment.");
        } else {
            space.as_mut_ptr() as *mut u8
        };

        ptr::copy_nonoverlapping(val as *const _ as *const u8, ptr_copy, size);

        Self {
            metadata,
            space: ManuallyDrop::new(space.assume_init()),
            _phantom: PhantomData,
        }
    }

    type Downcast = Copyable<U, Space>;
    unsafe fn downcast_unchecked(self) -> Self::Downcast
    where
        U: Sized,
    {
        let size = mem::size_of::<U>();
        let mut space = mem::MaybeUninit::<Space>::uninit();

        ptr::copy_nonoverlapping(
            &*self.space as *const _ as *const u8,
            space.as_mut_ptr() as *mut u8,
            size,
        );

        let metadata = ptr::metadata(space.as_ptr() as *const U);

        // No need to `mem::forget` as we're working with `Copy` types here.

        Copyable {
            metadata,
            space: ManuallyDrop::new(space.assume_init()),
            _phantom: Default::default(),
        }
    }
}

impl<T: ?Sized, Space: Copy> Storage<T> for Copyable<T, Space> {
    fn as_ptr(&self) -> *const T {
        ptr::from_raw_parts(&self.space as *const _ as *const (), self.metadata)
    }

    fn as_mut_ptr(&mut self) -> *mut T {
        ptr::from_raw_parts_mut(&mut self.space as *mut _ as *mut (), self.metadata)
    }
}

impl<T: ?Sized, Space: Storage<T> + Clone> Clone for MiniBox<T, Space> {
    fn clone(&self) -> Self {
        Self {
            space: self.space.clone(),
            _phantom: Default::default(),
        }
    }
}

impl<T: ?Sized, Space: Storage<T> + Copy> Copy for MiniBox<T, Space> {}

/// An "extra small box"! Stack-allocated storage for dynamically typed/"fat" objects.
///
/// In addition, if the storage space type is [`Copyable`], then the `MiniBox` can only store
/// [`Copy`] types but will itself be [`Copy`], even if it holds a `dyn Trait` object.
pub struct MiniBox<T: ?Sized, Space: Storage<T>> {
    space: Space,
    _phantom: PhantomData<T>,
}

impl<T: ?Sized, Space: Storage<T>> MiniBox<T, Space> {
    /// Construct a new [`MiniBox`].
    #[inline(always)]
    pub fn new(val: T) -> Self
    where
        T: Sized,
        Space: Storable<T, T>,
    {
        minibox!(val)
    }
}

impl<T: ?Sized, Space: Storage<T>> MiniBox<T, Space> {
    #[doc(hidden)]
    #[inline]
    pub unsafe fn new_unchecked<U>(val: U, ptr: *const T) -> Self
    where
        U: Sized,
        Space: Storable<T, U>,
    {
        let space = <Space as Storable<T, U>>::new_copy(&val, ptr);
        mem::forget(val);
        Self {
            space,
            _phantom: Default::default(),
        }
    }

    unsafe fn downcast_unchecked<U: Any>(self) -> MiniBox<U, <Space as Storable<T, U>>::Downcast>
    where
        Space: Storable<T, U>,
    {
        let space = self.space.downcast_unchecked();

        MiniBox {
            space,
            _phantom: Default::default(),
        }
    }

    /// Get an immutable pointer to the value inside the [`MiniBox`].
    #[inline]
    pub fn as_ptr(&self) -> *const T {
        self.space.as_ptr()
    }

    /// Get a mutable pointer to the value inside the [`MiniBox`].
    #[inline]
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.space.as_mut_ptr()
    }

    /// Consume the [`MiniBox`] and return the value which was stored inside.
    #[inline]
    pub fn into_inner(self) -> T
    where
        T: Sized,
    {
        let ret_val: T = unsafe { self.as_ptr().read() };
        mem::forget(self);
        ret_val
    }
}

impl<Space: Storage<dyn Any>> MiniBox<dyn Any, Space> {
    /// Attempt to downcast the [`MiniBox`] into a type which was stored inside it, by-value.
    #[inline]
    pub fn downcast<T: Any>(self) -> Result<MiniBox<T, Space::Downcast>, Self>
    where
        Space: Storable<dyn Any, T>,
    {
        if self.is::<T>() {
            unsafe { Ok(self.downcast_unchecked()) }
        } else {
            Err(self)
        }
    }
}

impl<Space: Storage<dyn Any + Send>> MiniBox<dyn Any + Send, Space> {
    /// Attempt to downcast the [`MiniBox`] into a type which was stored inside it, by-value.
    #[inline]
    pub fn downcast<T: Any>(self) -> Result<MiniBox<T, Space::Downcast>, Self>
    where
        Space: Storable<dyn Any + Send, T>,
    {
        if self.is::<T>() {
            unsafe { Ok(self.downcast_unchecked()) }
        } else {
            Err(self)
        }
    }
}

impl<T: ?Sized, Space: Storage<T>> Deref for MiniBox<T, Space> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.as_ptr() }
    }
}

impl<T: ?Sized, Space: Storage<T>> DerefMut for MiniBox<T, Space> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.as_mut_ptr() }
    }
}

impl<T: ?Sized + fmt::Display, Space: Storage<T>> fmt::Display for MiniBox<T, Space> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<T: ?Sized + fmt::Debug, Space: Storage<T>> fmt::Debug for MiniBox<T, Space> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: ?Sized, Space: Storage<T>> fmt::Pointer for MiniBox<T, Space> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let ptr: *const T = &**self;
        fmt::Pointer::fmt(&ptr, f)
    }
}

impl<T: ?Sized + PartialEq, Space: Storage<T>> PartialEq for MiniBox<T, Space> {
    fn eq(&self, other: &MiniBox<T, Space>) -> bool {
        PartialEq::eq(&**self, &**other)
    }

    #[allow(clippy::partialeq_ne_impl)]
    fn ne(&self, other: &MiniBox<T, Space>) -> bool {
        PartialEq::ne(&**self, &**other)
    }
}

impl<T: ?Sized + PartialOrd, Space: Storage<T>> PartialOrd for MiniBox<T, Space> {
    fn partial_cmp(&self, other: &MiniBox<T, Space>) -> Option<Ordering> {
        PartialOrd::partial_cmp(&**self, &**other)
    }

    fn lt(&self, other: &MiniBox<T, Space>) -> bool {
        PartialOrd::lt(&**self, &**other)
    }

    fn le(&self, other: &MiniBox<T, Space>) -> bool {
        PartialOrd::le(&**self, &**other)
    }

    fn ge(&self, other: &MiniBox<T, Space>) -> bool {
        PartialOrd::ge(&**self, &**other)
    }

    fn gt(&self, other: &MiniBox<T, Space>) -> bool {
        PartialOrd::gt(&**self, &**other)
    }
}

impl<T: ?Sized + Ord, Space: Storage<T>> Ord for MiniBox<T, Space> {
    fn cmp(&self, other: &MiniBox<T, Space>) -> Ordering {
        Ord::cmp(&**self, &**other)
    }
}

impl<T: ?Sized + Eq, Space: Storage<T>> Eq for MiniBox<T, Space> {}

impl<T: ?Sized + Hash, Space: Storage<T>> Hash for MiniBox<T, Space> {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}

unsafe impl<T: ?Sized + Send, Space: Storage<T> + Send> Send for MiniBox<T, Space> {}
unsafe impl<T: ?Sized + Sync, Space: Storage<T>> Sync for MiniBox<T, Space> {}
