//! Heavy Elastic - almost-safe abstractions for "stretching" lifetimes (and dealing with what
//! happens when they have to snap back.)
//!
//! This crate provides four main abstractions:
//! - [`Stretchable<'a>`], a trait indicating that some type with a lifetime `'a` can have that
//!   lifetime `'a` *removed* (and virtually set to `'static`.)
//! - [`Stretched`], a marker trait denoting that a type is a stretched version of a
//!   [`Stretchable<'a>`] type. It is unsafe to implement [`Stretched`]. Read the docs and do so at
//!   your own risk, or better yet, avoid doing so and use [`StretchedRef`] and [`StretchedMut`]
//!   instead.
//! - [`Elastic<T>`], a container for a stretched value (which may be empty.) It acts as an [`Arc`]
//!   and an [`AtomicRefCell`], by way of [`ArcCell`](hv_cell::ArcCell), and allows "loaning" the
//!   [`Stretchable`] type corresponding to its `T: Stretched`.
//! - [`ElasticGuard<'a, T>`], a guard which ensures that some loan to an [`Elastic<T::Stretched>`]
//!   doesn't outlive its original lifetime. When dropped, it forcibly takes the value back from the
//!   [`Elastic`] it was loaned from, and panics if doing so is impossible. You can also take the
//!   value back manually.

#![no_std]
#![warn(missing_docs)]
#![warn(missing_debug_implementations)]
#![feature(generic_associated_types)]

use core::{fmt, marker::PhantomData, ptr::NonNull};

// Used by `impl_stretched_methods`.
#[doc(hidden)]
pub use core::mem::transmute;

use hv_guarded_borrow::{
    NonBlockingGuardedBorrow, NonBlockingGuardedBorrowMut, NonBlockingGuardedMutBorrowMut,
};

use hv_cell::{ArcCell, ArcRef, ArcRefMut, AtomicRef, AtomicRefMut};

/// Small convenience macro for correctly implementing the four unsafe methods of [`Stretched`].
#[macro_export]
macro_rules! impl_stretched_methods {
    () => {
        unsafe fn lengthen(this: Self::Parameterized<'_>) -> Self {
            $crate::transmute(this)
        }

        unsafe fn shorten<'a>(this: Self) -> Self::Parameterized<'a> {
            $crate::transmute(this)
        }

        unsafe fn shorten_mut<'a>(this: &'_ mut Self) -> &'_ mut Self::Parameterized<'a> {
            $crate::transmute(this)
        }

        unsafe fn shorten_ref<'a>(this: &'_ Self) -> &'_ Self::Parameterized<'a> {
            $crate::transmute(this)
        }
    };
}

pub mod external;

/// Marker trait indicating that a type can be stretched (has a type for which there is an
/// implementation of `Stretched`, and which properly "translates back" with its `Parameterized`
/// associated type.)
pub trait Stretchable<'a>: 'a {
    /// The type you get by stretching `Self`.
    #[rustfmt::skip]
    type Stretched: Stretched<Parameterized<'a> = Self>;
}

/// A type which is a "stretched" version of a second type, representing the result of having every
/// single lifetime in that type set to `'static`.
///
/// # Safety
///
/// Holy shit this is incredibly fucking unsafe. It's cursed. It's so unbelievably cursed I'd like
/// to forget I wrote it but in all the circumstances I want to use it it should be safe so fine,
/// whatever. For the sake of completeness, however, I will say that there are *TWO major
/// requirements:*
///
/// Number one: ***DEPENDING ON YOUR TYPE, IT MAY BE UNDEFINED BEHAVIOR TO ACTUALLY HAVE IT
/// REPRESENTED WITH THE PARAMETERIZED LIFETIME SUBSTITUTED WITH `'static`!*** The Rust aliasing
/// rules state that if you turn a pointer to some `T` - which includes a reference to a `T` - into
/// a reference with a given lifetime... then ***you must respect the aliasing rules with respect to
/// that lifetime for the rest of the lifetime, even if you get rid of the value and it is no longer
/// touched!*** Instead of using `'static` lifetimes to represent a stretched thing, use pointers,
/// or - horror of horrors - implement `Stretched` for `pub StretchedMyType([u8;
/// std::mem::size_of::<MyType>()]);`. Yes, this will work, and it is safe/will not cause undefined
/// behavior, unlike having `&'static MyType` around and having Rust assume that the thing it
/// pointed to will never, ever be mutated again.
///
/// Number two: A type which is stretchable is parameterized over a lifetime. *It **must** be
/// covariant over that lifetime.* The reason for this is that essentially the `Stretched` trait and
/// [`StretchCell`] allow you to *decouple two lifetimes at a number of "decoupled reborrows".* The
/// first lifetime here is the lifetime of the original data, which is carried over in
/// [`StretchGuard`]; [`StretchGuard`] ensures that the data is dropped at or before the end of its
/// lifetime (and if it can't, everything will blow up with a panic.) The second lifetime is the
/// lifetime of every borrow from the [`StretchCell`]. As such what [`StretchCell`] and
/// [`Stretchable`] actually allow you to do is tell Rust to *assume* that the reborrowed lifetimes
/// are all outlived by the original lifetime, and blow up/error if not. This should scare you
/// shitless. However, I am unstoppable and I won't do what you tell me.
///
/// That is all. Godspeed.
pub unsafe trait Stretched: 'static + Sized {
    /// The parameterized type, which must be bit-equivalent to the unparameterized `Self` type. It
    /// must have the same size, same pointer size, same alignment, same *everything.*
    type Parameterized<'a>: Stretchable<'a, Stretched = Self>
    where
        Self: 'a;

    /// Lengthen the lifetime of a [`Stretched::Parameterized`] to `'static`.
    ///
    /// # Safety
    ///
    /// This is highly unsafe, and care must be taken to ensure that the lengthened data is taken
    /// care of and not discarded before the actual lifetime of the data. Most of the time this
    /// function is simply implemented as a wrapper around [`core::mem::transmute`]; this should give
    /// you a hint as to just how wildly unsafe this can be if mishandled.
    unsafe fn lengthen(this: Self::Parameterized<'_>) -> Self;

    /// Shorten the lifetime of a `'static` self to some arbitrary [`Stretched::Parameterized`].
    /// This is intended strictly as the inverse of [`Stretched::lengthen`], and makes no guarantees
    /// about its behavior if not used as such.
    ///
    /// # Safety
    ///
    /// Shortening a lifetime is normally totally safe, but this function might be usable in cases
    /// where the lifetime is actually invariant. In this case, it is extremely unsafe and care must
    /// be taken to ensure that the lifetime of the shortened data is the same as the lifetime of
    /// the data before its lifetime was lengthened. This function should be simply implemented as a
    /// wrapper around [`core::mem::transmute`]; this should give you a hint as to just how wildly
    /// unsafe this can be if mishandled.
    unsafe fn shorten<'a>(this: Self) -> Self::Parameterized<'a>;

    /// Equivalent to [`Stretched::shorten`] but operates on a mutable reference to the stretched
    /// type.
    ///
    /// # Safety
    ///
    /// Same as [`Stretched::shorten`]. Should be implemented simply as a wrapper around transmute.
    unsafe fn shorten_mut<'a>(this: &'_ mut Self) -> &'_ mut Self::Parameterized<'a>;

    /// Equivalent to [`Stretched::shorten`] but operates on an immutable reference to the stretched
    /// type.
    ///
    /// # Safety
    ///
    /// Same as [`Stretched::shorten`]. Should be implemented simply as a wrapper around transmute.
    unsafe fn shorten_ref<'a>(this: &'_ Self) -> &'_ Self::Parameterized<'a>;
}

/// A guard representing a loan of some stretchable value to some [`Elastic`].
pub struct ElasticGuard<'a, T: Stretchable<'a>> {
    slot: ArcCell<Option<T::Stretched>>,
    _phantom: PhantomData<fn(&'a ())>,
}

impl<'a, T: Stretchable<'a>> fmt::Debug for ElasticGuard<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ElasticGuard {{ .. }}")
    }
}

impl<'a, T: Stretchable<'a>> ElasticGuard<'a, T> {
    /// Revoke the loan and retrieve the loaned value.
    pub fn take(self) -> T {
        let stretched = self
            .slot
            .as_inner()
            .borrow_mut()
            .take()
            .expect("empty slot!");
        let shortened = unsafe { <T::Stretched>::shorten(stretched) };
        core::mem::forget(self);
        shortened
    }
}

impl<'a, T: Stretchable<'a>> Drop for ElasticGuard<'a, T> {
    fn drop(&mut self) {
        if let Some(stretched) = self.slot.as_inner().borrow_mut().take() {
            drop(unsafe { <T::Stretched>::shorten(stretched) });
        }
    }
}

/// A container for a stretched value.
///
/// This acts a bit like `Arc<AtomicRefCell<Option<T>>>`, through its `Clone` behavior and borrowing
/// methods, but the similarity ends there. The only way to put data into this type is through
/// [`Elastic::loan`], which allows you to safely loan some [`Stretchable`], non-`'static` type, to
/// an [`Elastic`] carrying the corresponding [`Stretched`] type. As [`Stretched`] requires
/// `'static`, [`Elastic<T>`] is always `'static` and can be used to access non-`'static` types
/// safely from contexts which require `'static` (such as dynamic typing with `Any` or the
/// `hv-alchemy` crate.) The lifetime is preserved by a scope guard, [`ElasticGuard`], which is
/// provided when a value is loaned and which revokes the loan when it is dropped or has the loaned
/// value forcibly taken back by [`ElasticGuard::take`].
///
/// [`Elastic<T>`] is thread-safe. However, internally, it uses an
/// [`AtomicRefCell`](hv_cell::AtomicRefCell), so if you violate borrowing invariants, you will have
/// a panic on your hands. This goes likewise for taking the value back w/ [`ElasticGuard`] or
/// dropping the guard: the guard will panic if it cannot take back the value.
///
/// # Soundness
///
/// As it comes to soundness, this crate currently will have issues with what might happen if a
/// thread tries to take back an elastic value from another thread which is currently borrowing it;
/// the owning thread will panic, and potentially end up dropping the borrowed value, while it is
/// being accessed by another thread. We may need to use an [`RwLock`] and block instead/use
/// something which supports poisoning... alternatively, report the error and directly abort.
#[derive(Debug)]
pub struct Elastic<T: Stretched> {
    slot: ArcCell<Option<T>>,
}

impl<T: Stretched> Clone for Elastic<T> {
    fn clone(&self) -> Self {
        Self {
            slot: self.slot.clone(),
        }
    }
}

impl<T: Stretched> Default for Elastic<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Stretched> Elastic<T> {
    /// Create an empty [`Elastic<T>`].
    pub fn new() -> Self {
        Self {
            slot: Default::default(),
        }
    }

    /// Attempt to immutably borrow the loaned value, if present. Returns `None` if nothing is
    /// currently loaned to this [`Elastic`].
    #[track_caller]
    pub fn borrow(&self) -> Option<AtomicRef<T::Parameterized<'_>>> {
        AtomicRef::filter_map(self.slot.as_inner().borrow(), Option::as_ref)
            .map(|arm| AtomicRef::map(arm, |t| unsafe { T::shorten_ref(t) }))
    }

    /// Attempt to mutably borrow the loaned value, if present. Returns `None` if nothing is
    /// currently loaned to this [`Elastic`].
    #[track_caller]
    pub fn borrow_mut(&self) -> Option<AtomicRefMut<T::Parameterized<'_>>> {
        AtomicRefMut::filter_map(self.slot.as_inner().borrow_mut(), Option::as_mut)
            .map(|arm| AtomicRefMut::map(arm, |t| unsafe { T::shorten_mut(t) }))
    }

    /// Attempt to immutably borrow the loaned value, via a reference-counted guard. Returns `None`
    /// if nothing is currently loaned to this [`Elastic`].
    #[track_caller]
    pub fn borrow_arc<'b, U: 'b, F>(&'b self, f: F) -> Option<ArcRef<U, Option<T>>>
    where
        F: for<'a> FnOnce(&'a T::Parameterized<'a>) -> &'a U,
    {
        ArcRef::filter_map(self.slot.borrow(), Option::as_ref)
            .map(|arc| ArcRef::map(arc, |t| f(unsafe { T::shorten_ref(t) })))
    }

    /// Attempt to mutably borrow the loaned value, via a reference-counted guard. Returns `None` if
    /// nothing is currently loaned to this [`Elastic`].
    #[track_caller]
    pub fn borrow_arc_mut<'b, U: 'b, F>(&'b mut self, f: F) -> Option<ArcRefMut<U, Option<T>>>
    where
        F: for<'a> FnOnce(&'a mut T::Parameterized<'a>) -> &'a mut U,
    {
        ArcRefMut::filter_map(self.slot.borrow_mut(), Option::as_mut)
            .map(|arc| ArcRefMut::map(arc, |t| f(unsafe { T::shorten_mut(t) })))
    }

    /// Loan a stretchable value to this [`Elastic`] in exchange for a guard object which ends the
    /// loan when the value is taken back or when the guard is dropped.
    ///
    /// Panics if there is already a loan in progress to this [`Elastic`].
    ///
    /// # Safety
    ///
    /// The guard *must* have its destructor run by the end of its lifetime, either by dropping it
    /// or using [`ElasticGuard::take`]. Calling [`core::mem::forget`] on an [`ElasticGuard`] is
    /// considered instant undefined behavior, as it leaves an [`Elastic`] in a state which is not
    /// well-defined and potentially contains a stretched value which is long past the end of its
    /// life, causing a use-after-free.
    #[track_caller]
    pub unsafe fn loan<'a>(
        &self,
        t: T::Parameterized<'a>,
    ) -> ElasticGuard<'a, T::Parameterized<'a>> {
        let mut slot = self.slot.as_inner().borrow_mut();
        assert!(
            slot.is_none(),
            "Elastic is already in the middle of a loan!"
        );
        let stretched = T::lengthen(t);
        *slot = Some(stretched);

        ElasticGuard {
            slot: self.slot.clone(),
            _phantom: PhantomData,
        }
    }
}

impl<T: Stretched, U: ?Sized> NonBlockingGuardedBorrow<U> for Elastic<T>
where
    for<'a> T::Parameterized<'a>: core::borrow::Borrow<U>,
{
    type Guard<'a>
    where
        U: 'a,
    = AtomicRef<'a, U>;
    type BorrowError<'a>
    where
        U: 'a,
    = ();

    fn try_nonblocking_guarded_borrow(&self) -> Result<Self::Guard<'_>, Self::BorrowError<'_>> {
        self.borrow()
            .ok_or(())
            .map(|guard| AtomicRef::map(guard, |t| core::borrow::Borrow::borrow(t)))
    }
}

impl<T: Stretched, U: ?Sized> NonBlockingGuardedBorrowMut<U> for Elastic<T>
where
    for<'a> T::Parameterized<'a>: core::borrow::BorrowMut<U>,
{
    type GuardMut<'a>
    where
        U: 'a,
    = AtomicRefMut<'a, U>;
    type BorrowMutError<'a>
    where
        U: 'a,
    = ();

    fn try_nonblocking_guarded_borrow_mut(
        &self,
    ) -> Result<Self::GuardMut<'_>, Self::BorrowMutError<'_>> {
        self.borrow_mut()
            .ok_or(())
            .map(|guard| AtomicRefMut::map(guard, |t| core::borrow::BorrowMut::borrow_mut(t)))
    }
}

impl<T: Stretched, U: ?Sized> NonBlockingGuardedMutBorrowMut<U> for Elastic<T>
where
    for<'a> T::Parameterized<'a>: core::borrow::BorrowMut<U>,
{
    type MutGuardMut<'a>
    where
        U: 'a,
    = AtomicRefMut<'a, U>;
    type MutBorrowMutError<'a>
    where
        U: 'a,
    = ();

    fn try_nonblocking_guarded_mut_borrow_mut(
        &mut self,
    ) -> Result<Self::MutGuardMut<'_>, Self::MutBorrowMutError<'_>> {
        self.borrow_mut()
            .ok_or(())
            .map(|guard| AtomicRefMut::map(guard, |t| core::borrow::BorrowMut::borrow_mut(t)))
    }
}

/// A type representing a stretched `&T` reference. Has the same representation as a `*const T`.
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct StretchedRef<T: ?Sized>(*const T);

unsafe impl<T: Sync> Send for StretchedRef<T> {}
unsafe impl<T: Sync> Sync for StretchedRef<T> {}

/// A type representing a stretched `&mut T` reference. Has the same representation as a `*mut T`.
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct StretchedMut<T: ?Sized>(NonNull<T>);

unsafe impl<T: Send> Send for StretchedMut<T> {}
unsafe impl<T: Sync> Sync for StretchedMut<T> {}

unsafe impl<T: 'static> Stretched for StretchedRef<T> {
    type Parameterized<'a> = &'a T;

    unsafe fn lengthen(this: Self::Parameterized<'_>) -> Self {
        core::mem::transmute(this)
    }

    unsafe fn shorten<'a>(this: Self) -> Self::Parameterized<'a> {
        &*this.0.cast()
    }

    unsafe fn shorten_mut<'a>(this: &'_ mut Self) -> &'_ mut Self::Parameterized<'a> {
        core::mem::transmute(this)
    }

    unsafe fn shorten_ref<'a>(this: &'_ Self) -> &'_ Self::Parameterized<'a> {
        core::mem::transmute(this)
    }
}

impl<'a, T: 'static> Stretchable<'a> for &'a T {
    type Stretched = StretchedRef<T>;
}

unsafe impl<T: 'static> Stretched for StretchedMut<T> {
    type Parameterized<'a> = &'a mut T;

    unsafe fn lengthen(this: Self::Parameterized<'_>) -> Self {
        core::mem::transmute(this)
    }

    unsafe fn shorten<'a>(this: Self) -> Self::Parameterized<'a> {
        this.0.cast().as_mut()
    }

    unsafe fn shorten_mut<'a>(this: &'_ mut Self) -> &'_ mut Self::Parameterized<'a> {
        core::mem::transmute(this)
    }

    unsafe fn shorten_ref<'a>(this: &'_ Self) -> &'_ Self::Parameterized<'a> {
        core::mem::transmute(this)
    }
}

impl<'a, T: 'static> Stretchable<'a> for &'a mut T {
    type Stretched = StretchedMut<T>;
}

macro_rules! impl_tuple {
    ($($letter:ident),*) => {
        unsafe impl<$($letter: Stretched,)*> Stretched for ($($letter,)*) {
            type Parameterized<'a> = ($(<$letter as Stretched>::Parameterized<'a>,)*);

            #[allow(non_snake_case, clippy::unused_unit)]
            unsafe fn lengthen(this: ($(<$letter as Stretched>::Parameterized<'_>,)*)) -> Self {
                let ($($letter,)*) = this;
                ($($letter::lengthen($letter),)*)
            }

            #[allow(non_snake_case, clippy::unused_unit)]
            unsafe fn shorten<'a>(this: Self) -> Self::Parameterized<'a> {
                let ($($letter,)*) = this;
                ($($letter::shorten($letter),)*)
            }

            unsafe fn shorten_mut<'a>(this: &'_ mut Self) -> &'_ mut Self::Parameterized<'a> {
                core::mem::transmute(this)
            }

            unsafe fn shorten_ref<'a>(this: &'_ Self) -> &'_ Self::Parameterized<'a> {
                core::mem::transmute(this)
            }
        }

        impl<'a, $($letter: Stretchable<'a>,)*> Stretchable<'a> for ($($letter,)*) {
            type Stretched = ($(<$letter as Stretchable<'a>>::Stretched,)*);
        }
    };
}

macro_rules! russian_tuples {
    ($m: ident, $ty: tt) => {
        $m!{}
        $m!{$ty}
    };
    ($m: ident, $ty: tt, $($tt: tt),*) => {
        russian_tuples!{$m, $($tt),*}
        $m!{$ty, $($tt),*}
    };
}

russian_tuples!(impl_tuple, A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);

unsafe impl<T: Stretched> Stretched for Option<T> {
    type Parameterized<'a> = Option<T::Parameterized<'a>>;

    unsafe fn lengthen(this: Self::Parameterized<'_>) -> Self {
        this.map(|t| unsafe { T::lengthen(t) })
    }

    unsafe fn shorten<'a>(this: Self) -> Self::Parameterized<'a> {
        this.map(|t| unsafe { T::shorten(t) })
    }

    unsafe fn shorten_mut<'a>(this: &'_ mut Self) -> &'_ mut Self::Parameterized<'a> {
        core::mem::transmute(this)
    }

    unsafe fn shorten_ref<'a>(this: &'_ Self) -> &'_ Self::Parameterized<'a> {
        core::mem::transmute(this)
    }
}

impl<'a, T: Stretchable<'a>> Stretchable<'a> for Option<T> {
    type Stretched = Option<T::Stretched>;
}
