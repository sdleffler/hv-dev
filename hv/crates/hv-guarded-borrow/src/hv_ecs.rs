use std::convert::Infallible;

use crate::{NonBlockingGuardedBorrow, NonBlockingGuardedMutBorrowMut};

impl<T> NonBlockingGuardedBorrow<T> for hv_ecs::DynamicComponent<T> {
    type Guard<'a>
    = hv_ecs::DynamicItemRef<'a, T> where T: 'a;
    type BorrowError<'a>
    = Infallible where T: 'a;

    fn try_nonblocking_guarded_borrow(&self) -> Result<Self::Guard<'_>, Self::BorrowError<'_>> {
        Ok(self.borrow())
    }
}

impl<T> NonBlockingGuardedMutBorrowMut<T> for hv_ecs::DynamicComponent<T> {
    type MutGuardMut<'a>
    = hv_ecs::DynamicItemRefMut<'a, T> where T: 'a;
    type MutBorrowMutError<'a>
    = Infallible where T: 'a;

    fn try_nonblocking_guarded_mut_borrow_mut(
        &mut self,
    ) -> Result<Self::MutGuardMut<'_>, Self::MutBorrowMutError<'_>> {
        Ok(self.borrow_mut())
    }
}
