//! [`Stretched`] implementations and types for the [`hecs`] crate.

use core::fmt;
use core::marker::PhantomData;

use crate::{Stretchable, Stretched};

/// The type of a stretched [`BatchWriter`].
#[repr(C, align(8))]
pub struct StretchedBatchWriter<T>(
    [u8; core::mem::size_of::<hecs::BatchWriter<u8>>()],
    PhantomData<T>,
);

impl<T> fmt::Debug for StretchedBatchWriter<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StretchedBatchWriter(..)")
    }
}

static_assertions::assert_eq_size!(StretchedBatchWriter<u32>, hecs::BatchWriter<u32>);
static_assertions::assert_eq_align!(StretchedBatchWriter<u32>, hecs::BatchWriter<u32>);

unsafe impl<T: 'static> Stretched for StretchedBatchWriter<T> {
    type Parameterized<'a> = hecs::BatchWriter<'a, T>;

    impl_stretched_methods!();
}

impl<'a, T: 'static> Stretchable<'a> for hecs::BatchWriter<'a, T> {
    type Stretched = StretchedBatchWriter<T>;
}
