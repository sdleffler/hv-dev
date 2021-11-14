//! [`Stretched`] implementations and types for the [`hv_ecs`] crate.

use core::fmt;

use crate::{Stretchable, Stretched};

/// The type of a stretched [`BatchWriter`].
pub struct StretchedBatchWriter<T: 'static>(
    [u8; core::mem::size_of::<hv_ecs::BatchWriter<u8>>()],
    [hv_ecs::BatchWriter<'static, T>; 0],
);

impl<T> fmt::Debug for StretchedBatchWriter<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StretchedBatchWriter(..)")
    }
}

static_assertions::assert_eq_size!(StretchedBatchWriter<u32>, hv_ecs::BatchWriter<u32>);
static_assertions::assert_eq_align!(StretchedBatchWriter<u32>, hv_ecs::BatchWriter<u32>);

unsafe impl<T: 'static> Stretched for StretchedBatchWriter<T> {
    type Parameterized<'a> = hv_ecs::BatchWriter<'a, T>;

    impl_stretched_methods!();
}

impl<'a, T: 'static> Stretchable<'a> for hv_ecs::BatchWriter<'a, T> {
    type Stretched = StretchedBatchWriter<T>;
}
