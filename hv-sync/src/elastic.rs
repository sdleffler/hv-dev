use core::marker::PhantomData;

use crate::cell::{ArcCell, AtomicRef, AtomicRefMut};

pub trait Stretchable<'a>: 'a {
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
/// whatever. For the sake of completeness, however, I will say that there is *one major
/// requirement:*
///
/// A type which is stretchable is parameterized over a lifetime. *It **must** be covariant over
/// that lifetime.* The reason for this is that essentially the `Stretched` trait and
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
pub unsafe trait Stretched:
    'static + Sized + Stretchable<'static, Stretched = Self>
{
    type Parameterized<'a>: Stretchable<'a, Stretched = Self>;

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
    /// the data before its lifetime was lengthened. Most of the time this function is simply
    /// implemented as a wrapper around [`core::mem::transmute`]; this should give you a hint as to
    /// just how wildly unsafe this can be if mishandled.
    unsafe fn shorten<'a>(this: Self) -> Self::Parameterized<'a>;

    /// Equivalent to [`Stretched::shorten`] but operates on a mutable reference to the stretched
    /// type.
    ///
    /// # Safety
    ///
    /// Same as [`Stretched::shorten`].
    unsafe fn shorten_mut<'a>(this: &'_ mut Self) -> &'_ mut Self::Parameterized<'a>;

    /// Equivalent to [`Stretched::shorten`] but operates on an immutable reference to the stretched
    /// type.
    ///
    /// # Safety
    ///
    /// Same as [`Stretched::shorten`].
    unsafe fn shorten_ref<'a>(this: &'_ Self) -> &'_ Self::Parameterized<'a>;
}

#[macro_export]
macro_rules! impl_stretched_methods {
    () => {
        unsafe fn lengthen(this: Self::Parameterized<'_>) -> Self {
            core::mem::transmute(this)
        }

        unsafe fn shorten<'a>(this: Self) -> Self::Parameterized<'a> {
            core::mem::transmute(this)
        }

        unsafe fn shorten_mut<'a>(this: &'_ mut Self) -> &'_ mut Self::Parameterized<'a> {
            core::mem::transmute(this)
        }

        unsafe fn shorten_ref<'a>(this: &'_ Self) -> &'_ Self::Parameterized<'a> {
            core::mem::transmute(this)
        }
    };
}

unsafe impl<T> Stretched for &'static T {
    type Parameterized<'a> = &'a T;

    unsafe fn lengthen(this: Self::Parameterized<'_>) -> Self {
        core::mem::transmute(this)
    }

    unsafe fn shorten<'a>(this: Self) -> Self::Parameterized<'a> {
        core::mem::transmute(this)
    }

    unsafe fn shorten_mut<'a>(this: &'_ mut Self) -> &'_ mut Self::Parameterized<'a> {
        core::mem::transmute(this)
    }

    unsafe fn shorten_ref<'a>(this: &'_ Self) -> &'_ Self::Parameterized<'a> {
        core::mem::transmute(this)
    }
}

impl<'a, T: 'static> Stretchable<'a> for &'a T {
    type Stretched = &'static T;
}

unsafe impl<T> Stretched for &'static mut T {
    type Parameterized<'a> = &'a mut T;

    unsafe fn lengthen(this: Self::Parameterized<'_>) -> Self {
        core::mem::transmute(this)
    }

    unsafe fn shorten<'a>(this: Self) -> Self::Parameterized<'a> {
        core::mem::transmute(this)
    }

    unsafe fn shorten_mut<'a>(this: &'_ mut Self) -> &'_ mut Self::Parameterized<'a> {
        core::mem::transmute(this)
    }

    unsafe fn shorten_ref<'a>(this: &'_ Self) -> &'_ Self::Parameterized<'a> {
        core::mem::transmute(this)
    }
}

impl<'a, T: 'static> Stretchable<'a> for &'a mut T {
    type Stretched = &'static mut T;
}

pub struct ElasticGuard<'a, T: Stretchable<'a>> {
    slot: ArcCell<Option<T::Stretched>>,
    _phantom: PhantomData<fn(&'a ())>,
}

impl<'a, T: Stretchable<'a>> ElasticGuard<'a, T> {
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

#[derive(Debug)]
pub struct Elastic<T: Stretched> {
    slot: ArcCell<Option<T>>,
}

impl<T: Stretched> Default for Elastic<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Stretched> Elastic<T> {
    pub fn new() -> Self {
        Self {
            slot: Default::default(),
        }
    }

    #[track_caller]
    pub fn borrow(&self) -> Option<AtomicRef<T::Parameterized<'_>>> {
        AtomicRef::filter_map(self.slot.as_inner().borrow(), Option::as_ref)
            .map(|arm| AtomicRef::map(arm, |t| unsafe { <T::Stretched>::shorten_ref(t) }))
    }

    #[track_caller]
    pub fn borrow_mut(&self) -> Option<AtomicRefMut<T::Parameterized<'_>>> {
        AtomicRefMut::filter_map(self.slot.as_inner().borrow_mut(), Option::as_mut)
            .map(|arm| AtomicRefMut::map(arm, |t| unsafe { <T::Stretched>::shorten_mut(t) }))
    }

    #[track_caller]
    pub fn loan<'a>(&self, t: T::Parameterized<'a>) -> ElasticGuard<'a, T::Parameterized<'a>> {
        let mut slot = self.slot.borrow_mut();
        assert!(slot.is_none(), "stretchcell already in use!");
        let stretched = unsafe { T::lengthen(t) };
        *slot = Some(stretched);

        ElasticGuard {
            slot: self.slot.clone(),
            _phantom: PhantomData,
        }
    }
}
