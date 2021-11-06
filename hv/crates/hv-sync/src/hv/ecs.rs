use core::marker::PhantomData;

use crate::elastic::{Stretchable, Stretched};

#[repr(transparent)]
pub struct StretchedBatchWriter<T>(
    [u8; core::mem::size_of::<hecs::BatchWriter<u8>>()],
    PhantomData<T>,
);

unsafe impl<T: 'static> Stretched for StretchedBatchWriter<T> {
    type Parameterized<'a> = hecs::BatchWriter<'a, T>;

    impl_stretched_methods!();
}

impl<'a, T: 'static> Stretchable<'a> for hecs::BatchWriter<'a, T> {
    type Stretched = StretchedBatchWriter<T>;
}
