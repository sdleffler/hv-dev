use std::sync::{
    Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard, TryLockError,
};

use crate::{
    NonBlockingGuardedBorrow, NonBlockingGuardedBorrowMut, NonBlockingGuardedMutBorrowMut,
};

impl<T: ?Sized> NonBlockingGuardedBorrow<T> for Mutex<T> {
    type Guard<'a>
    where
        T: 'a,
    = MutexGuard<'a, T>;
    type BorrowError<'a>
    where
        T: 'a,
    = TryLockError<MutexGuard<'a, T>>;

    fn try_nonblocking_guarded_borrow(&self) -> Result<Self::Guard<'_>, Self::BorrowError<'_>> {
        self.try_lock()
    }
}

impl<T: ?Sized> NonBlockingGuardedBorrowMut<T> for Mutex<T> {
    type GuardMut<'a>
    where
        T: 'a,
    = MutexGuard<'a, T>;
    type BorrowMutError<'a>
    where
        T: 'a,
    = TryLockError<MutexGuard<'a, T>>;

    fn try_nonblocking_guarded_borrow_mut(
        &self,
    ) -> Result<Self::GuardMut<'_>, Self::BorrowMutError<'_>> {
        self.try_lock()
    }
}

impl<T: ?Sized> NonBlockingGuardedMutBorrowMut<T> for Mutex<T> {
    type MutGuardMut<'a>
    where
        T: 'a,
    = &'a mut T;
    type MutBorrowMutError<'a>
    where
        T: 'a,
    = PoisonError<&'a mut T>;

    fn try_nonblocking_guarded_mut_borrow_mut(
        &mut self,
    ) -> Result<Self::MutGuardMut<'_>, Self::MutBorrowMutError<'_>> {
        self.get_mut()
    }
}

impl<T: ?Sized> NonBlockingGuardedBorrow<T> for RwLock<T> {
    type Guard<'a>
    where
        T: 'a,
    = RwLockReadGuard<'a, T>;
    type BorrowError<'a>
    where
        T: 'a,
    = TryLockError<RwLockReadGuard<'a, T>>;

    fn try_nonblocking_guarded_borrow(&self) -> Result<Self::Guard<'_>, Self::BorrowError<'_>> {
        self.try_read()
    }
}

impl<T: ?Sized> NonBlockingGuardedBorrowMut<T> for RwLock<T> {
    type GuardMut<'a>
    where
        T: 'a,
    = RwLockWriteGuard<'a, T>;
    type BorrowMutError<'a>
    where
        T: 'a,
    = TryLockError<RwLockWriteGuard<'a, T>>;

    fn try_nonblocking_guarded_borrow_mut(
        &self,
    ) -> Result<Self::GuardMut<'_>, Self::BorrowMutError<'_>> {
        self.try_write()
    }
}

impl<T: ?Sized> NonBlockingGuardedMutBorrowMut<T> for RwLock<T> {
    type MutGuardMut<'a>
    where
        T: 'a,
    = &'a mut T;
    type MutBorrowMutError<'a>
    where
        T: 'a,
    = PoisonError<&'a mut T>;

    fn try_nonblocking_guarded_mut_borrow_mut(
        &mut self,
    ) -> Result<Self::MutGuardMut<'_>, Self::MutBorrowMutError<'_>> {
        self.get_mut()
    }
}
