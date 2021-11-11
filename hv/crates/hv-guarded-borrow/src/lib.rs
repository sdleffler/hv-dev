#![feature(generic_associated_types)]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{boxed::Box, rc::Rc, sync::Arc};
use core::ops::{Deref, DerefMut};
use core::{
    cell::{BorrowError, BorrowMutError, Ref, RefCell, RefMut},
    convert::Infallible,
};

#[cfg(feature = "hecs")]
mod hecs;

#[cfg(feature = "std")]
mod std;

pub trait NonBlockingGuardedBorrow<T: ?Sized> {
    type Guard<'a>: Deref<Target = T>
    where
        T: 'a,
        Self: 'a;
    type BorrowError<'a>
    where
        T: 'a,
        Self: 'a;

    fn try_nonblocking_guarded_borrow(&self) -> Result<Self::Guard<'_>, Self::BorrowError<'_>>;
}

pub trait NonBlockingGuardedBorrowMut<T: ?Sized> {
    type GuardMut<'a>: Deref<Target = T> + DerefMut
    where
        T: 'a,
        Self: 'a;
    type BorrowMutError<'a>
    where
        T: 'a,
        Self: 'a;

    fn try_nonblocking_guarded_borrow_mut(
        &self,
    ) -> Result<Self::GuardMut<'_>, Self::BorrowMutError<'_>>;
}

pub trait NonBlockingGuardedMutBorrowMut<T: ?Sized> {
    type MutGuardMut<'a>: Deref<Target = T> + DerefMut
    where
        T: 'a,
        Self: 'a;
    type MutBorrowMutError<'a>
    where
        T: 'a,
        Self: 'a;

    fn try_nonblocking_guarded_mut_borrow_mut(
        &mut self,
    ) -> Result<Self::MutGuardMut<'_>, Self::MutBorrowMutError<'_>>;
}

impl<'a, T: ?Sized> NonBlockingGuardedBorrow<T> for &'a T {
    type Guard<'b>
    where
        T: 'b,
        Self: 'b,
    = &'b T;
    type BorrowError<'b>
    where
        T: 'b,
        Self: 'b,
    = Infallible;

    fn try_nonblocking_guarded_borrow(&self) -> Result<Self::Guard<'_>, Self::BorrowError<'_>> {
        Ok(self)
    }
}

impl<'a, T: ?Sized> NonBlockingGuardedBorrowMut<T> for &'a T {
    type GuardMut<'b>
    where
        T: 'b,
        Self: 'b,
    = &'b mut T;
    type BorrowMutError<'b>
    where
        T: 'b,
        Self: 'b,
    = &'static str;

    fn try_nonblocking_guarded_borrow_mut(
        &self,
    ) -> Result<Self::GuardMut<'_>, Self::BorrowMutError<'_>> {
        Err("cannot mutably borrow from behind a shared reference")
    }
}

impl<'a, T: ?Sized> NonBlockingGuardedMutBorrowMut<T> for &'a T {
    type MutGuardMut<'b>
    where
        T: 'b,
        Self: 'b,
    = &'b mut T;
    type MutBorrowMutError<'b>
    where
        T: 'b,
        Self: 'b,
    = &'static str;

    fn try_nonblocking_guarded_mut_borrow_mut(
        &mut self,
    ) -> Result<Self::MutGuardMut<'_>, Self::MutBorrowMutError<'_>> {
        Err("cannot mutably borrow from behind a shared reference")
    }
}

impl<'a, T: ?Sized> NonBlockingGuardedBorrow<T> for &'a mut T {
    type Guard<'b>
    where
        T: 'b,
        Self: 'b,
    = &'b T;
    type BorrowError<'b>
    where
        T: 'b,
        Self: 'b,
    = Infallible;

    fn try_nonblocking_guarded_borrow(&self) -> Result<Self::Guard<'_>, Self::BorrowError<'_>> {
        Ok(self)
    }
}

impl<'a, T: ?Sized> NonBlockingGuardedBorrowMut<T> for &'a mut T {
    type GuardMut<'b>
    where
        T: 'b,
        Self: 'b,
    = &'b mut T;
    type BorrowMutError<'b>
    where
        T: 'b,
        Self: 'b,
    = &'static str;

    fn try_nonblocking_guarded_borrow_mut(
        &self,
    ) -> Result<Self::GuardMut<'_>, Self::BorrowMutError<'_>> {
        Err("cannot mutably borrow from behind a shared reference")
    }
}

impl<'a, T: ?Sized> NonBlockingGuardedMutBorrowMut<T> for &'a mut T {
    type MutGuardMut<'b>
    where
        T: 'b,
        Self: 'b,
    = &'b mut T;
    type MutBorrowMutError<'b>
    where
        T: 'b,
        Self: 'b,
    = Infallible;

    fn try_nonblocking_guarded_mut_borrow_mut(
        &mut self,
    ) -> Result<Self::MutGuardMut<'_>, Self::MutBorrowMutError<'_>> {
        Ok(self)
    }
}

impl<T: ?Sized> NonBlockingGuardedBorrow<T> for RefCell<T> {
    type Guard<'a>
    where
        T: 'a,
    = Ref<'a, T>;
    type BorrowError<'a>
    where
        T: 'a,
    = BorrowError;

    fn try_nonblocking_guarded_borrow(&self) -> Result<Self::Guard<'_>, Self::BorrowError<'_>> {
        self.try_borrow()
    }
}

impl<T: ?Sized> NonBlockingGuardedBorrowMut<T> for RefCell<T> {
    type GuardMut<'a>
    where
        T: 'a,
    = RefMut<'a, T>;
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

impl<T: ?Sized> NonBlockingGuardedMutBorrowMut<T> for RefCell<T> {
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

impl<T: ?Sized, U: ?Sized> NonBlockingGuardedBorrow<U> for Rc<T>
where
    T: NonBlockingGuardedBorrow<U>,
{
    type Guard<'a>
    where
        U: 'a,
        Self: 'a,
    = T::Guard<'a>;
    type BorrowError<'a>
    where
        U: 'a,
        Self: 'a,
    = T::BorrowError<'a>;

    fn try_nonblocking_guarded_borrow(&self) -> Result<Self::Guard<'_>, Self::BorrowError<'_>> {
        T::try_nonblocking_guarded_borrow(self)
    }
}

impl<T: ?Sized, U: ?Sized> NonBlockingGuardedBorrowMut<U> for Rc<T>
where
    T: NonBlockingGuardedBorrowMut<U>,
{
    type GuardMut<'a>
    where
        U: 'a,
        Self: 'a,
    = T::GuardMut<'a>;
    type BorrowMutError<'a>
    where
        U: 'a,
        Self: 'a,
    = T::BorrowMutError<'a>;

    fn try_nonblocking_guarded_borrow_mut(
        &self,
    ) -> Result<Self::GuardMut<'_>, Self::BorrowMutError<'_>> {
        T::try_nonblocking_guarded_borrow_mut(self)
    }
}

impl<T: ?Sized, U: ?Sized> NonBlockingGuardedMutBorrowMut<U> for Rc<T>
where
    T: NonBlockingGuardedBorrowMut<U>,
{
    type MutGuardMut<'a>
    where
        U: 'a,
        Self: 'a,
    = T::GuardMut<'a>;
    type MutBorrowMutError<'a>
    where
        U: 'a,
        Self: 'a,
    = T::BorrowMutError<'a>;

    fn try_nonblocking_guarded_mut_borrow_mut(
        &mut self,
    ) -> Result<Self::MutGuardMut<'_>, Self::MutBorrowMutError<'_>> {
        T::try_nonblocking_guarded_borrow_mut(self)
    }
}

impl<T: ?Sized, U: ?Sized> NonBlockingGuardedBorrow<U> for Arc<T>
where
    T: NonBlockingGuardedBorrow<U>,
{
    type Guard<'a>
    where
        U: 'a,
        Self: 'a,
    = T::Guard<'a>;
    type BorrowError<'a>
    where
        U: 'a,
        Self: 'a,
    = T::BorrowError<'a>;

    fn try_nonblocking_guarded_borrow(&self) -> Result<Self::Guard<'_>, Self::BorrowError<'_>> {
        T::try_nonblocking_guarded_borrow(self)
    }
}

impl<T: ?Sized, U: ?Sized> NonBlockingGuardedBorrowMut<U> for Arc<T>
where
    T: NonBlockingGuardedBorrowMut<U>,
{
    type GuardMut<'a>
    where
        U: 'a,
        Self: 'a,
    = T::GuardMut<'a>;
    type BorrowMutError<'a>
    where
        U: 'a,
        Self: 'a,
    = T::BorrowMutError<'a>;

    fn try_nonblocking_guarded_borrow_mut(
        &self,
    ) -> Result<Self::GuardMut<'_>, Self::BorrowMutError<'_>> {
        T::try_nonblocking_guarded_borrow_mut(self)
    }
}

impl<T: ?Sized, U: ?Sized> NonBlockingGuardedMutBorrowMut<U> for Arc<T>
where
    T: NonBlockingGuardedBorrowMut<U>,
{
    type MutGuardMut<'a>
    where
        U: 'a,
        Self: 'a,
    = T::GuardMut<'a>;
    type MutBorrowMutError<'a>
    where
        U: 'a,
        Self: 'a,
    = T::BorrowMutError<'a>;

    fn try_nonblocking_guarded_mut_borrow_mut(
        &mut self,
    ) -> Result<Self::MutGuardMut<'_>, Self::MutBorrowMutError<'_>> {
        T::try_nonblocking_guarded_borrow_mut(self)
    }
}

impl<T: ?Sized, U: ?Sized> NonBlockingGuardedBorrow<U> for Box<T>
where
    T: NonBlockingGuardedBorrow<U>,
{
    type Guard<'a>
    where
        U: 'a,
        Self: 'a,
    = T::Guard<'a>;
    type BorrowError<'a>
    where
        U: 'a,
        Self: 'a,
    = T::BorrowError<'a>;

    fn try_nonblocking_guarded_borrow(&self) -> Result<Self::Guard<'_>, Self::BorrowError<'_>> {
        T::try_nonblocking_guarded_borrow(self)
    }
}

impl<T: ?Sized, U: ?Sized> NonBlockingGuardedBorrowMut<U> for Box<T>
where
    T: NonBlockingGuardedBorrowMut<U>,
{
    type GuardMut<'a>
    where
        U: 'a,
        Self: 'a,
    = T::GuardMut<'a>;
    type BorrowMutError<'a>
    where
        U: 'a,
        Self: 'a,
    = T::BorrowMutError<'a>;

    fn try_nonblocking_guarded_borrow_mut(
        &self,
    ) -> Result<Self::GuardMut<'_>, Self::BorrowMutError<'_>> {
        T::try_nonblocking_guarded_borrow_mut(self)
    }
}

impl<T: ?Sized, U: ?Sized> NonBlockingGuardedMutBorrowMut<U> for Box<T>
where
    T: NonBlockingGuardedMutBorrowMut<U>,
{
    type MutGuardMut<'a>
    where
        U: 'a,
        Self: 'a,
    = T::MutGuardMut<'a>;
    type MutBorrowMutError<'a>
    where
        U: 'a,
        Self: 'a,
    = T::MutBorrowMutError<'a>;

    fn try_nonblocking_guarded_mut_borrow_mut(
        &mut self,
    ) -> Result<Self::MutGuardMut<'_>, Self::MutBorrowMutError<'_>> {
        T::try_nonblocking_guarded_mut_borrow_mut(self)
    }
}
