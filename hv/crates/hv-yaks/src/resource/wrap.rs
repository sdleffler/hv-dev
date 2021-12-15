use super::{AtomicBorrow, ResourceCell};

pub trait ResourceWrapElem {
    type Elem;

    fn wrap_elem(&mut self, borrows: &mut AtomicBorrow) -> ResourceCell<Self::Elem>;
}

impl<R0> ResourceWrapElem for &'_ R0 {
    type Elem = R0;

    fn wrap_elem(&mut self, borrow: &mut AtomicBorrow) -> ResourceCell<Self::Elem> {
        ResourceCell::new_shared(self, borrow)
    }
}

impl<R0> ResourceWrapElem for &'_ mut R0 {
    type Elem = R0;

    fn wrap_elem(&mut self, borrow: &mut AtomicBorrow) -> ResourceCell<Self::Elem> {
        ResourceCell::new_mut(self, borrow)
    }
}

/// Specifies how a tuple of references is wrapped into a tuple of cells.
pub trait ResourceWrap {
    type Wrapped;
    type BorrowTuple;

    fn wrap(&mut self, borrows: &mut Self::BorrowTuple) -> Self::Wrapped;
}

impl ResourceWrap for () {
    type Wrapped = ();
    type BorrowTuple = ();

    fn wrap(&mut self, _: &mut Self::BorrowTuple) -> Self::Wrapped {}
}

impl<'a, R0> ResourceWrap for &'a R0 {
    type Wrapped = (ResourceCell<R0>,);
    type BorrowTuple = (AtomicBorrow,);

    fn wrap(&mut self, borrows: &mut Self::BorrowTuple) -> Self::Wrapped {
        (ResourceCell::new_shared(self, &mut borrows.0),)
    }
}

impl<'a, R0> ResourceWrap for &'a mut R0 {
    type Wrapped = (ResourceCell<R0>,);
    type BorrowTuple = (AtomicBorrow,);

    fn wrap(&mut self, borrows: &mut Self::BorrowTuple) -> Self::Wrapped {
        (ResourceCell::new_mut(self, &mut borrows.0),)
    }
}

impl<R0> ResourceWrap for (&'_ R0,) {
    type Wrapped = (ResourceCell<R0>,);
    type BorrowTuple = (AtomicBorrow,);

    fn wrap(&mut self, borrows: &mut Self::BorrowTuple) -> Self::Wrapped {
        (ResourceCell::new_shared(self.0, &mut borrows.0),)
    }
}

impl<R0> ResourceWrap for (&'_ mut R0,) {
    type Wrapped = (ResourceCell<R0>,);
    type BorrowTuple = (AtomicBorrow,);

    fn wrap(&mut self, borrows: &mut Self::BorrowTuple) -> Self::Wrapped {
        (ResourceCell::new_mut(self.0, &mut borrows.0),)
    }
}

macro_rules! swap_to_atomic_borrow {
    ($anything:tt) => {
        AtomicBorrow
    };
    (new $anything:tt) => {
        AtomicBorrow::new()
    };
}

macro_rules! impl_resource_wrap {
    ($($letter:ident),*) => {
        paste::item! {
            impl<$($letter),*> ResourceWrap for ($($letter,)*)
                where $($letter: ResourceWrapElem),*
            {
                type Wrapped = ($(ResourceCell<$letter::Elem>,)*);
                type BorrowTuple = ($(swap_to_atomic_borrow!($letter),)*);

                #[allow(non_snake_case)]
                fn wrap(&mut self, borrows: &mut Self::BorrowTuple) -> Self::Wrapped {
                    let ($([<S $letter>],)*) = self;
                    let ($([<B $letter>],)*) = borrows;
                    ($( ResourceWrapElem::wrap_elem([<S $letter>], [<B $letter>]) ,)*)
                }
            }
        }
    }
}

impl_for_tuples!(impl_resource_wrap);
