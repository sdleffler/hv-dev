use std::sync::{
    Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard, TryLockError,
};

use crate::{
    NonBlockingGuardedBorrow, NonBlockingGuardedBorrowMut, NonBlockingGuardedMutBorrowMut,
};

impl<T: ?Sized> NonBlockingGuardedBorrow<T> for Mutex<T> {
    type Guard<'a>
    = MutexGuard<'a, T> where T: 'a,;
    type BorrowError<'a>
    = TryLockError<MutexGuard<'a, T>> where T: 'a;

    fn try_nonblocking_guarded_borrow(&self) -> Result<Self::Guard<'_>, Self::BorrowError<'_>> {
        self.try_lock()
    }
}

impl<T: ?Sized> NonBlockingGuardedBorrowMut<T> for Mutex<T> {
    type GuardMut<'a>
    = MutexGuard<'a, T> where T: 'a;
    type BorrowMutError<'a>
    = TryLockError<MutexGuard<'a, T>> where T: 'a;

    fn try_nonblocking_guarded_borrow_mut(
        &self,
    ) -> Result<Self::GuardMut<'_>, Self::BorrowMutError<'_>> {
        self.try_lock()
    }
}

impl<T: ?Sized> NonBlockingGuardedMutBorrowMut<T> for Mutex<T> {
    type MutGuardMut<'a>
    = &'a mut T where T: 'a;
    type MutBorrowMutError<'a>
    = PoisonError<&'a mut T> where T: 'a;

    fn try_nonblocking_guarded_mut_borrow_mut(
        &mut self,
    ) -> Result<Self::MutGuardMut<'_>, Self::MutBorrowMutError<'_>> {
        self.get_mut()
    }
}

impl<T: ?Sized> NonBlockingGuardedBorrow<T> for RwLock<T> {
    type Guard<'a>
    = RwLockReadGuard<'a, T> where T: 'a;
    type BorrowError<'a>
    = TryLockError<RwLockReadGuard<'a, T>> where T: 'a;

    fn try_nonblocking_guarded_borrow(&self) -> Result<Self::Guard<'_>, Self::BorrowError<'_>> {
        self.try_read()
    }
}

impl<T: ?Sized> NonBlockingGuardedBorrowMut<T> for RwLock<T> {
    type GuardMut<'a>
    = RwLockWriteGuard<'a, T> where T: 'a;
    type BorrowMutError<'a>
    = TryLockError<RwLockWriteGuard<'a, T>> where T: 'a;

    fn try_nonblocking_guarded_borrow_mut(
        &self,
    ) -> Result<Self::GuardMut<'_>, Self::BorrowMutError<'_>> {
        self.try_write()
    }
}

impl<T: ?Sized> NonBlockingGuardedMutBorrowMut<T> for RwLock<T> {
    type MutGuardMut<'a>
    = &'a mut T where T: 'a;
    type MutBorrowMutError<'a>
    = PoisonError<&'a mut T> where T: 'a;

    fn try_nonblocking_guarded_mut_borrow_mut(
        &mut self,
    ) -> Result<Self::MutGuardMut<'_>, Self::MutBorrowMutError<'_>> {
        self.get_mut()
    }
}
