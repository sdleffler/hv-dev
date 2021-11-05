use std::convert::Infallible;

use crate::{NonBlockingGuardedBorrow, NonBlockingGuardedMutBorrowMut};

impl<T> NonBlockingGuardedBorrow<T> for hecs::DynamicComponent<T> {
    type Guard<'a>
    where
        T: 'a,
    = hecs::DynamicItemRef<'a, T>;
    type BorrowError<'a>
    where
        T: 'a,
    = Infallible;

    fn try_nonblocking_guarded_borrow(&self) -> Result<Self::Guard<'_>, Self::BorrowError<'_>> {
        Ok(self.borrow())
    }
}

impl<T> NonBlockingGuardedMutBorrowMut<T> for hecs::DynamicComponent<T> {
    type MutGuardMut<'a>
    where
        T: 'a,
    = hecs::DynamicItemRefMut<'a, T>;
    type MutBorrowMutError<'a>
    where
        T: 'a,
    = Infallible;

    fn try_nonblocking_guarded_mut_borrow_mut(
        &mut self,
    ) -> Result<Self::MutGuardMut<'_>, Self::MutBorrowMutError<'_>> {
        Ok(self.borrow_mut())
    }
}
