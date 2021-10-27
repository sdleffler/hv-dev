use crate::elastic::{Stretchable, Stretched};

impl<'a, T: 'static> Stretchable<'a> for hecs::BatchWriter<'a, T> {
    type Stretched = hecs::BatchWriter<'static, T>;
}

unsafe impl<T> Stretched for hecs::BatchWriter<'static, T> {
    type Parameterized<'a> = hecs::BatchWriter<'a, T>;

    impl_stretched_methods!();
}
