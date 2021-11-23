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
//!
//! Requires nightly for `#![feature(generic_associated_types, allocator_api)]`.
//!
//! # Why would I want this?
//!
//! [`Elastic`] excels at "re-loaning" objects across "`'static` boundaries". For example, the Rust
//! standard library's [`std::thread::spawn`] function requires that the [`FnOnce`] closure you use
//! to start a new thread is `Send + 'static`. I'm calling this a "`'static` boundary" because it
//! effectively partitions your code into two different sets of lifetimes - the lifetimes on the
//! parent thread, and the lifetimes on the child thread, and you're forced to separate these
//! because of a `'static` bound on the closure. So what happens if you need to send over a
//! reference to something which isn't `'static`? Without unsafe abstractions or refactoring to
//! remove the lifetime (which in some cases won't be possible because the type isn't from your
//! crate in the first place) you're, generally speaking, screwed. [`Elastic`] lets you get around
//! this problem by providing a "slot" which can have a value safely and remotely loaned to it.
//!
//! ## Using [`Elastic`] for crossing `Send + 'static` boundaries
//!
//! Let's first look at the problem without [`Elastic`]:
//!
//! ```compile_fail
//! # use core::cell::RefCell;
//! let my_special_u32 = RefCell::new(23);
//!
//! // This fails for two reasons: first, RefCell is not Sync, so it's unsafe to Send any kind
//! // of reference to it across a thread boundary. Second, the `&'a mut RefCell<u32>` created
//! // implicitly by this closure borrowing its environment is non-static.
//! std::thread::spawn(|| {
//!     *my_special_u32.borrow_mut() *= 3;
//! }).join().unwrap();
//!
//! // We know that the thread will have returned by now thanks to the use of `.join()`, but
//! // the borrowchecker has no way of proving that! Which is why it requires the closure to be
//! // static in the first place.
//! let important_computed_value = *my_special_u32.borrow() * 6 + 6;
//! ```
//!
//! If you're stuck with a [`RefCell<T>`], it may be hard to see a way to get a mutable reference to
//! its contained value across a combined `'static + Send` boundary. However, with [`Elastic`], you
//! can deal with the situation cleanly, no matter where the `&'a mut T` comes from:
//!
//! ```
//! # use hv_elastic::{ElasticMut, ScopeArena};
//! # use core::cell::RefCell;
//! # let my_special_u32 = RefCell::new(23);
//! // Create an empty "scope arena", which we need for allocating scope guards.
//! let mut scope_arena = ScopeArena::new();
//! // Create an empty elastic which expects to be loaned an `&'a mut u32`.
//! let empty_elastic = ElasticMut::<u32>::new();
//!
//! {
//!     // Elastics are shared, using an Arc under the hood. Cloning them is cheap
//!     // and does not clone any data inside.
//!     let shared_elastic = empty_elastic.clone();
//!     let mut refcell_guard = my_special_u32.borrow_mut();
//!     scope_arena.scope(|guard| {
//!         // If you can get an `&'a mut T` out of it, you can loan it to an `ElasticMut<T>`.
//!         guard.loan(&shared_elastic, &mut *refcell_guard);
//!
//!         // Spawn a thread to do some computation with our borrowed u32, and take ownership of
//!         // the clone we made of our Elastic.
//!         std::thread::spawn(move || {
//!             // Internally, `Elastic` contains an atomic refcell. This is necessary for safety,
//!             // so unfortunately we have to suck it up and take the extra verbosity.
//!             *shared_elastic.borrow_mut() *= 3;
//!         }).join().unwrap();
//!
//!         // At the end of this scope, the borrowed reference is forcibly removed from the
//!         // shared Elastic, and the lifetime bounds on `scope` ensure that the refcell guard is
//!         // still valid at this point. However, the value inside the refcell has long since been
//!         // modified by our spawned thread!
//!     });
//!
//!     // Now, the refcell guard drops.
//! }
//!
//! // The elastic never took ownership of the refcell or the value inside in any way - it was
//! // temporarily loaned an `&'a mut u32` which came from inside a `core::cell::RefMut<'a, u32>`
//! // and did any modifications directly on that reference. So code "after" any elastic
//! // manipulation looks exactly the same as before - no awkward wrappers or anything.
//! let important_computed_value = *my_special_u32.borrow() * 6 + 6;
//!
//! // With the current design of Elastic, the scope arena will not automagically release the memory
//! // it allocated, so if it's used in a loop, you'll probably want to call `.reset()` occasionally
//! // to release the memory used to allocate the scope guards:
//! scope_arena.reset();
//! ```
//!
//! ## Using [`Elastic`] for enabling dynamic typing with non-`'static` values
//!
//! Rust's [`core::any::Any`] trait is an invaluable tool for writing code which doesn't always have
//! static knowledge of the types involved. However, when it comes to lifetimes, the question of how
//! to handle dynamic typing is complex and unresolved. Should a `Foo<'a>` have a different type ID
//! from a `Foo<'static>`? As the thread discussing this is the greatest thread in the history of
//! forums, locked by a moderator after 12,239 pages of heated debate, (it was not and I am not
//! aware of any such thread; this is a joke) [`Any`](core::any::Any) has a `'static` bound on it.
//! Which is very inconvenient if what you want to do is completely *ignore* any lifetimes in your
//! dynamically typed code by treating them as if they're all equal (and/or `'static`.)
//!
//! Elastic can help here. If the type you want to stick into an `Any` is an `&T` or `&mut T`, it's
//! very straightforward as [`ElasticRef<T>`] and [`ElasticMut<T>`] are both `'static`. If the type
//! you have is not a plain old reference, it's a bit nastier; you need to ensure the safety of
//! lifetime manipulation on the type in question, and then manually construct a type which
//! represents (as plain old data) a lifetime-erased version of that type. Then, manually implement
//! [`Stretched`] and [`Stretchable`] on it. This is ***highly*** unsafe! Please take special care
//! to respect the requirements on implementations of [`Stretchable`]. Size and alignment of the
//! stretched type must match. This is pretty much the only requirement though. The
//! [`impl_stretched_methods`] macro exists to help you safely implement the methods required by
//! [`Stretched`] once you ensure the lifetimes are correct. Just note that it does this by swinging
//! a giant hammer named [`core::mem::transmute`], and it does this *by design*, and that **if you
//! screw up on the lifetime safety requirements you are headed on a one way trip to
//! Undefinedbehaviortown.**
//!
//! # Yes, there are twelve different ways to borrow from an [`Elastic`], and every single one is useful
//!
//! How may I borrow from thee? Well, let me count the ways:
//!
//! ## Dereferenced borrows (the kind you want most of the time)
//!
//! Eight of the borrow methods are "dereferenced" - they expect you to be stretching references or
//! smart pointers/guards with lifetimes in them. If you're using [`ElasticRef`]/[`ElasticMut`] or
//! [`StretchedRef`]/[`StretchedMut`], these are what you want; they're much more convenient than
//! the parameterized borrows.
//!
//! - [`Elastic::borrow`] and [`Elastic::borrow_mut`]; most of the time, if you're working with
//!   references being extended, and you don't care about handling borrow errors, you'll use these.
//!   99% of the time, they do what you want, and you're probably going to be enforcing invariants
//!   to make sure it wouldn't error anyways. These are the [`Elastic`] versions of
//!   [`RefCell::borrow`] and [`RefCell::borrow_mut`], and they pretty much behave identicaly.
//! - [`Elastic::borrow_arc`] and [`Elastic::borrow_arc_mut`]; these come from the [`ArcCell`] which
//!   lives inside an [`Elastic`], and offer guards juts like `borrow` and `borrow_mut` *but*, those
//!   guards are reference counted and don't have a lifetime attached. So instead of
//!   [`AtomicRef<'a, T>`], you get [`ArcRef<T>`], which has the same lifetime as `T`... and if `T`
//!   is `'static`, so is [`ArcRef<T>`]/[`ArcRefMut<T>`], which can be very useful, again for
//!   passing across `Send`/`'static`/whatnot boundaries.
//! - [`Elastic::try_borrow`], [`Elastic::try_borrow_mut`], [`Elastic::try_borrow_arc`],
//!   [`Elastic::try_borrow_arc_mut`]; these are just versions of the four
//!   `borrow`/`borrow_mut`/`borrow_arc`/`borrow_arc_mut` methods which don't panic on failure, and
//!   return `Result` instead.
//!
//! ## Parameterized borrows (you're in the deep end, now)
//!
//! The last four methods are what the other eight are all implemented on. These return `Result`
//! instead of panicking, and provide direct access to whatever [`T::Parameterized<'a>`] is. In the
//! case of [`StretchedRef`] and [`StretchedMut`], we have `<StretchedRef<T>>::Parameterized<'a> =
//! &'a T` and `<StretchedMut<T>>::Parameterized<'a> = &'a mut T`; when we use a method like
//! [`Elastic::try_borrow_as_parameterized_mut`] on an [`Elastic<StretchedMut<T>>`], we'll get back
//! `Result<AtomicRefMut<'_, &'_ mut T>, BorrowMutError>` which is pretty obviously redundant. It's
//! for this reason that the other eight methods exist to handle the common cases and abstract away
//! the fact that [`Elastic`] is more than just an [`AtomicRefCell`].
//!
//! # Safety: [`ElasticGuard`], [`ScopeGuard`] and [`ScopeArena`]
//!
//! [`Elastic`] works by erasing the lifetime on the type and then leaving you with an
//! [`ElasticGuard<'a>`] which preserves the lifetime. This [`ElasticGuard`] is a "drop guard" - in
//! its [`Drop`] implementation, it tries to take back the loaned value, preventing it from being
//! used after the loan expires. There are a couple ways this can go wrong:
//!
//! 1. If [`Elastic`] is currently borrowed when the guard drops, a panic will occur, because the
//!    guard's drop impl needs mutable access to the "slot" inside the [`Elastic`].
//! 2. If you [`core::mem::forget`] the [`ElasticGuard`], the slot inside the [`Elastic`] will never
//!    be cleared, which is *highly* unsafe, as you now have a stretched value running around which
//!    is no longer bounded by any lifetime. This is a recipe for undefined behavior and
//!    use-after-free bugs.
//!
//! The first error case is unavoidable; if you're loaning stuff out, you might have to drop
//! something while it's in use. To avoid this, loaning should be done in phases; loan a bunch of
//! things at once, ensure whatever is using those loans finishes, and then expire those loans.
//! Thankfully, the solution to making this easy *and* avoiding the possibility of the second
//! failure mode can exist in one primitive: [`ScopeArena`]. [`ScopeArena`] provides a method
//! [`ScopeArena::scope`], which allows you to create scopes in which a [`ScopeGuard`] takes
//! ownership of the [`ElasticGuard`]s produced by the loaning operation. Since the [`ScopeGuard`]
//! is owned by the caller - [`ScopeArena::scope`] - the user cannot accidentally or intentionally
//! [`core::mem::forget`] a guard, and in addition, the guard ensures that all of the loans made to
//! it have the same parameterized lifetime, which encourages the phase-loaning pattern.
//!
//! In short, *always use [`ScopeArena`] and [`ScopeGuard`] - if you think you have to use
//! [`ElasticGuard`] for some reason, double check!*
//!
//! [`Stretchable<'a>`]: crate::Stretchable
//! [`Arc`]: alloc::sync::Arc
//! [`RefCell<T>`]: core::cell::RefCell
//! [`AtomicRefCell`]: hv_cell::AtomicRefCell
//! [`AtomicRef<'a, T>`]: hv_cell::AtomicRef
//! [`ElasticGuard<'a, T>`]: crate::ElasticGuard

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(missing_debug_implementations)]
#![feature(generic_associated_types, allocator_api)]

extern crate alloc;

use core::{
    fmt,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

// Used by `impl_stretched_methods`.
#[doc(hidden)]
pub use core::mem::transmute;

use alloc::vec::Vec;
use hv_guarded_borrow::{
    NonBlockingGuardedBorrow, NonBlockingGuardedBorrowMut, NonBlockingGuardedMutBorrowMut,
};

use hv_cell::{ArcCell, ArcRef, ArcRefMut, AtomicRef, AtomicRefMut};
use hv_stampede::{boxed::Box as ArenaBox, Bump};

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

#[cfg(any(feature = "hv-ecs"))]
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
/// ## Requirement #1: Do not use `'static` or references to represent your stretched type!
///
/// ***DEPENDING ON YOUR TYPE, IT MAY BE UNDEFINED BEHAVIOR TO ACTUALLY HAVE IT REPRESENTED WITH THE
/// PARAMETERIZED LIFETIME SUBSTITUTED WITH `'static`!*** The Rust aliasing rules state that if you
/// turn a pointer to some `T` - which includes a reference to a `T` - into a reference with a given
/// lifetime... then ***you must respect the aliasing rules with respect to that lifetime for the
/// rest of the lifetime, even if you get rid of the value and it is no longer touched!*** Instead
/// of using `'static` lifetimes to represent a stretched thing, use pointers, or - horror of
/// horrors - implement `Stretched` for something like:
///
/// ```
/// # #![feature(generic_associated_types)]
/// # use hv_elastic::{Stretched, Stretchable};
/// # pub struct MyType<'a>(&'a ());
///
/// pub struct StretchedMyType {
///     // An array of bytes gives us the exact size of the type we're stretching.
///     _data: [u8; std::mem::size_of::<MyType>()],
///     // And, a zero-sized array of a `'static` version of the type we're stretching gives us
///     // the required alignment. Because it's a zero-sized array, no values of the type
///     // `MyType<'static>` actually end up existing, so it's safe to use `'static` here.
///     _force_align: [MyType<'static>; 0],
/// }
///
/// // It is recommended to use `static_assertions` and always follow a definition like this with
/// // assertions that the alignment and size match, as required by the `Stretched` trait.
/// static_assertions::assert_eq_align!(MyType<'static>, StretchedMyType);
/// static_assertions::assert_eq_size!(MyType<'static>, StretchedMyType);
///
/// unsafe impl Stretched for StretchedMyType {
///     type Parameterized<'a> = MyType<'a>;
///
///     hv_elastic::impl_stretched_methods!();
/// }
///
/// impl<'a> Stretchable<'a> for MyType<'a> {
///     type Stretched = StretchedMyType;
/// }
/// ```
///
/// This creates a piece of plain old data with the same size and byte alignment as your type. Yes,
/// this will work. And yes, it is a much safer option/will not cause undefined behavior, unlike
/// having `&'static MyType` around and having Rust assume that the thing it pointed to will never,
/// ever be mutated again. **DO NOT NEEDLESSLY ANTAGONIZE THE RUST COMPILER! THE CRAB WILL NOT
/// FORGIVE YOU!**
///
/// ## Requirement #2: Your stretchable type must be covariant over the parameterized lifetime!
///
/// A type which is stretchable is parameterized over a lifetime. *It **must** be covariant over
/// that lifetime.* The reason for this is that essentially the `Stretched` trait and [`Elastic`]
/// allow you to *decouple two lifetimes at a number of "decoupled reborrows".* The first lifetime
/// here is the lifetime of the original data, which is carried over in [`ElasticGuard`];
/// [`ElasticGuard`] ensures that the data is dropped at or before the end of its lifetime (and if
/// it can't, everything will blow up with a panic.) The second lifetime is the lifetime of every
/// borrow from the [`Elastic`]. As such what [`Elastic`] and [`Stretchable`] actually allow you to
/// do is tell Rust to *assume* that the reborrowed lifetimes are all outlived by the original
/// lifetime, and blow up/error if not. This should scare you shitless. However, I am unstoppable
/// and I won't do what you tell me.
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

/// The error returned when an immutable borrow fails.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "std", derive(thiserror::Error))]
pub enum BorrowError {
    /// Couldn't borrow because the elastic was already mutably borrowed somewhere.
    #[cfg_attr(feature = "std", error("the elastic is already mutably borrowed"))]
    MutablyBorrowed,
    /// Couldn't borrow because the elastic either hadn't been loaned to, or any outstanding loaned
    /// values were already repossessed by destroying their [`ElasticGuard`].
    #[cfg_attr(feature = "std", error("the elastic is empty; it has not been loaned to, or any outstanding loan has been repossessed"))]
    EmptySlot,
}

/// The error returned when a mutable borrow fails.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "std", derive(thiserror::Error))]
pub enum BorrowMutError {
    /// Couldn't borrow because the elastic was already borrowed somewhere.
    #[cfg_attr(feature = "std", error("the elastic is already borrowed"))]
    Borrowed,
    /// Couldn't borrow because the elastic either hadn't been loaned to, or any outstanding loaned
    /// values were already repossessed by destroying their [`ElasticGuard`].
    #[cfg_attr(feature = "std", error("the elastic is empty; it has not been loaned to, or any outstanding loan has been repossessed"))]
    EmptySlot,
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
pub struct Elastic<T: Stretched> {
    slot: ArcCell<Option<T>>,
}

impl<T: Stretched> fmt::Debug for Elastic<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Elastic {{ ... }}")
    }
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

    /// Immutably borrow the loaned value. Panicks if the elastic is already mutably borrowed or if
    /// it was never loaned to/any loans have expired.
    ///
    /// This method assumes that the stretchable type is a reference or smart pointer and can be
    /// immediately dereferenced; if that's not the case, please use `try_borrow_as_parameterized`.
    #[track_caller]
    pub fn borrow<'a>(&'a self) -> AtomicRef<'a, <T::Parameterized<'a> as Deref>::Target>
    where
        T::Parameterized<'a>: Deref,
    {
        self.try_borrow().unwrap()
    }

    /// Mutably borrow the loaned value. Panicks if the elastic is already borrowed or if it was
    /// never loaned to/any loans have expired.
    ///
    /// This method assumes that the stretchable type is a reference or smart pointer and can be
    /// immediately dereferenced; if that's not the case, please use
    /// `try_borrow_as_parameterized_mut`.
    #[track_caller]
    pub fn borrow_mut<'a>(&'a self) -> AtomicRefMut<'a, <T::Parameterized<'a> as Deref>::Target>
    where
        T::Parameterized<'a>: DerefMut,
    {
        self.try_borrow_mut().unwrap()
    }

    /// Immutably borrow the loaned value through a reference-counted guard. Panicks if the
    /// elastic is already mutably borrowed or if it was never loaned to/any loans have expired.
    ///
    /// This method assumes that the stretchable type is a reference or smart pointer and can be
    /// immediately dereferenced; if that's not the case, please use `try_borrow_as_parameterized_arc`.
    #[track_caller]
    pub fn borrow_arc<'a>(&'a self) -> ArcRef<<T::Parameterized<'a> as Deref>::Target, Option<T>>
    where
        T::Parameterized<'a>: Deref,
    {
        self.try_borrow_arc().unwrap()
    }

    /// Mutably borrow the loaned value through a reference-counted guard. Panicks if the
    /// elastic is already borrowed or if it was never loaned to/any loans have expired.
    ///
    /// This method assumes that the stretchable type is a reference or smart pointer and can be
    /// immediately dereferenced; if that's not the case, please use
    /// `try_borrow_as_parameterized_arc_mut`.
    #[track_caller]
    pub fn borrow_arc_mut<'a>(
        &'a self,
    ) -> ArcRefMut<<T::Parameterized<'a> as Deref>::Target, Option<T>>
    where
        T::Parameterized<'a>: DerefMut,
    {
        self.try_borrow_arc_mut().unwrap()
    }

    /// Immutably borrow the loaned value. Returns `Err` if the elastic is already mutably borrowed
    /// or if it was never loaned to/any loans have expired.
    ///
    /// This method assumes that the stretchable type is a reference or smart pointer and can be
    /// immediately dereferenced; if that's not the case, please use `try_borrow_as_parameterized`.
    #[track_caller]
    pub fn try_borrow<'a>(
        &'a self,
    ) -> Result<AtomicRef<'a, <T::Parameterized<'a> as Deref>::Target>, BorrowError>
    where
        T::Parameterized<'a>: Deref,
    {
        let guard = self.try_borrow_as_parameterized()?;
        Ok(AtomicRef::map(guard, |t| &**t))
    }

    /// Immutably borrow the loaned value through a reference-counted guard. Returns `Err` if the
    /// elastic is already mutably borrowed or if it was never loaned to/any loans have expired.
    ///
    /// This method assumes that the stretchable type is a reference or smart pointer and can be
    /// immediately dereferenced; if that's not the case, please use `try_borrow_as_parameterized_arc`.
    #[track_caller]
    pub fn try_borrow_arc<'a>(
        &'a self,
    ) -> Result<ArcRef<<T::Parameterized<'a> as Deref>::Target, Option<T>>, BorrowError>
    where
        T::Parameterized<'a>: Deref,
    {
        let arc_ref = self.try_borrow_as_parameterized_arc()?;
        Ok(ArcRef::map(arc_ref, |t| &**t))
    }

    /// Mutably borrow the loaned value through a reference-counted guard. Returns `Err` if the
    /// elastic is already borrowed or if it was never loaned to/any loans have expired.
    ///
    /// This method assumes that the stretchable type is a reference or smart pointer and can be
    /// immediately dereferenced; if that's not the case, please use
    /// `try_borrow_as_parameterized_arc_mut`.
    #[track_caller]
    pub fn try_borrow_arc_mut<'a>(
        &'a self,
    ) -> Result<ArcRefMut<<T::Parameterized<'a> as Deref>::Target, Option<T>>, BorrowMutError>
    where
        T::Parameterized<'a>: DerefMut,
    {
        let arc_mut = self.try_borrow_as_parameterized_arc_mut()?;
        Ok(ArcRefMut::map(arc_mut, |t| &mut **t))
    }

    /// Mutably borrow the loaned value. Returns `Err` if the elastic is already borrowed or if it
    /// was never loaned to/any loans have expired.
    ///
    /// This method assumes that the stretchable type is a refence or smart pointer and can be
    /// immediately dereferenced; if that's not the case, please use
    /// `try_borrow_as_parameterized_mut`.
    #[track_caller]
    pub fn try_borrow_mut<'a>(
        &'a self,
    ) -> Result<AtomicRefMut<'a, <T::Parameterized<'a> as Deref>::Target>, BorrowMutError>
    where
        T::Parameterized<'a>: DerefMut,
    {
        let guard = self.try_borrow_as_parameterized_mut()?;
        Ok(AtomicRefMut::map(guard, |t| &mut **t))
    }

    /// Attempt to immutably borrow the loaned value, if present.
    #[track_caller]
    pub fn try_borrow_as_parameterized(
        &self,
    ) -> Result<AtomicRef<T::Parameterized<'_>>, BorrowError> {
        let guard = self
            .slot
            .as_inner()
            .try_borrow()
            .map_err(|_| BorrowError::MutablyBorrowed)?;
        AtomicRef::filter_map(guard, Option::as_ref)
            .map(|arm| AtomicRef::map(arm, |t| unsafe { T::shorten_ref(t) }))
            .ok_or(BorrowError::EmptySlot)
    }

    /// Attempt to mutably borrow the loaned value, if present.
    #[track_caller]
    pub fn try_borrow_as_parameterized_mut(
        &self,
    ) -> Result<AtomicRefMut<T::Parameterized<'_>>, BorrowMutError> {
        let guard = self
            .slot
            .as_inner()
            .try_borrow_mut()
            .map_err(|_| BorrowMutError::Borrowed)?;
        AtomicRefMut::filter_map(guard, Option::as_mut)
            .map(|arm| AtomicRefMut::map(arm, |t| unsafe { T::shorten_mut(t) }))
            .ok_or(BorrowMutError::EmptySlot)
    }

    /// Attempt to immutably borrow the loaned value, via a reference-counted guard.
    #[track_caller]
    pub fn try_borrow_as_parameterized_arc(
        &self,
    ) -> Result<ArcRef<T::Parameterized<'_>, Option<T>>, BorrowError> {
        let guard = self
            .slot
            .try_borrow()
            .map_err(|_| BorrowError::MutablyBorrowed)?;
        ArcRef::filter_map(guard, Option::as_ref)
            .map(|arc| ArcRef::map(arc, |t| unsafe { T::shorten_ref(t) }))
            .ok_or(BorrowError::EmptySlot)
    }

    /// Attempt to mutably borrow the loaned value, via a reference-counted guard..
    #[track_caller]
    pub fn try_borrow_as_parameterized_arc_mut(
        &self,
    ) -> Result<ArcRefMut<T::Parameterized<'_>, Option<T>>, BorrowMutError> {
        let guard = self
            .slot
            .try_borrow_mut()
            .map_err(|_| BorrowMutError::Borrowed)?;
        ArcRefMut::filter_map(guard, Option::as_mut)
            .map(|arc| ArcRefMut::map(arc, |t| unsafe { T::shorten_mut(t) }))
            .ok_or(BorrowMutError::EmptySlot)
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
    = BorrowError;

    fn try_nonblocking_guarded_borrow(&self) -> Result<Self::Guard<'_>, Self::BorrowError<'_>> {
        self.try_borrow_as_parameterized()
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
    = BorrowMutError;

    fn try_nonblocking_guarded_borrow_mut(
        &self,
    ) -> Result<Self::GuardMut<'_>, Self::BorrowMutError<'_>> {
        self.try_borrow_as_parameterized_mut()
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
    = BorrowMutError;

    fn try_nonblocking_guarded_mut_borrow_mut(
        &mut self,
    ) -> Result<Self::MutGuardMut<'_>, Self::MutBorrowMutError<'_>> {
        self.try_borrow_as_parameterized_mut()
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

/// An arena for allocating [`ScopeGuard`]s.
///
/// Functions as a memory pool for allocating various trait objects needed for dropping type-erased
/// [`ElasticGuard`]s. Each time the `scope` method is called, the arena will have some allocations
/// made; to free these, [`ScopeArena::reset`] should be called.
#[derive(Debug, Default)]
pub struct ScopeArena {
    bump: Bump,
}

impl ScopeArena {
    /// Create an empty scope arena.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a scope within which we can safely loan elastics.
    ///
    /// This method *does not* reset the arena afterwards, so if you use it, it is your
    /// responsibility to reset the `ScopeArena` with [`ScopeArena::reset`] to avoid memory leaks.
    pub fn scope<'a, F, R>(&'a self, f: F) -> R
    where
        F: FnOnce(&mut ScopeGuard<'a>) -> R,
    {
        let mut scope_guard = ScopeGuard {
            bump: &self.bump,
            buf: Vec::new_in(&self.bump),
            _phantom: PhantomData,
        };
        f(&mut scope_guard)
    }

    /// Clear memory allocated by the arena, preserving the allocations for reuse.
    pub fn reset(&mut self) {
        self.bump.reset();
    }
}

trait MakeItDyn {}

impl<T: ?Sized> MakeItDyn for T {}

/// A guard which allows "stashing" [`ElasticGuard`]s for safe loaning.
///
/// Bare [`Elastic::loan`] is unsafe, because the returned [`ElasticGuard`] *must* be dropped. A
/// [`ScopeGuard`] provided by [`ScopeArena::scope`] or [`ScopeArena::scope_mut`] allows for
/// collecting [`ElasticGuard`]s through its *safe* [`ScopeGuard::loan`] method, because the
/// [`ScopeArena`] ensures that all loans are ended at the end of the scope.
///
/// As an aside, [`ScopeGuard`] is an excellent example of a type which *cannot* be safely
/// stretched: the lifetime parameter of the [`ScopeGuard`] corresponds to the accepted lifetime on
/// the type parameter of [`ScopeGuard::loan`]. As a result, effectively, [`ScopeGuard`] has the
/// variance of `fn(&'a mut ...)`; it is safe to *lengthen* the lifetimes fed to [`ScopeGuard`], but
/// absolutely not safe to shorten them!
pub struct ScopeGuard<'a> {
    bump: &'a Bump,
    buf: Vec<ArenaBox<'a, (dyn MakeItDyn + 'a)>, &'a Bump>,
    // Ensure contravariance.
    _phantom: PhantomData<fn(&'a mut ())>,
}

impl<'a> fmt::Debug for ScopeGuard<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ScopeGuard({} guards held)", self.buf.len())
    }
}

impl<'a> ScopeGuard<'a> {
    /// Loan to an elastic within the lifetime of the scope guard.
    pub fn loan<T: Stretchable<'a>>(&mut self, elastic: &Elastic<T::Stretched>, value: T) {
        let boxed_dyn_guard = unsafe {
            let boxed_guard = ArenaBox::new_in(elastic.loan(value), self.bump);
            let raw_box = ArenaBox::into_raw(boxed_guard);
            <ArenaBox<'a, (dyn MakeItDyn + 'a)>>::from_raw(raw_box as *mut (dyn MakeItDyn + 'a))
        };
        self.buf.push(boxed_dyn_guard);
    }
}

/// An [`Elastic`] which is specialized for the task of loaning `&'a T`s. This is a type synonym for
/// `Elastic<StretchedRef<T>>`, and provides some more convenient methods adapted to dealing with
/// elastics of immutable references.
pub type ElasticRef<T> = Elastic<StretchedRef<T>>;

/// An [`Elastic`] which is specialized for the task of loaning `&'a mut T`s. This is a type synonym
/// for `Elastic<StretchedMut<T>>`, and provides some more convenient methods adapted to dealing
/// with elastics of mutable references.
pub type ElasticMut<T> = Elastic<StretchedMut<T>>;
