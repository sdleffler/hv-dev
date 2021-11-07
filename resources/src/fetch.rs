use std::{
    any::type_name,
    error::Error,
    fmt::{Display, Formatter, Result as FmtResult},
};

use crate::{
    error::CantGetResource,
    map::{Resource, Resources},
    refs::{Ref, RefMut},
};

/// Error that may occur when retrieving one or several of [`Resource`]
/// from a [`Resources`] container via [`::fetch()`].
///
/// [`Resource`]: trait.Resource.html
/// [`Resources`]: struct.Resources.html
/// [`::fetch()`]: struct.Resources.html#method.fetch
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CantFetch {
    /// Compiler-provided name of the type that encountered the error.
    pub type_name: &'static str,
    /// Specific cause of the error.
    pub cause: CantGetResource,
}

impl Display for CantFetch {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "cannot fetch {}: {}", self.type_name, self.cause)
    }
}

impl Error for CantFetch {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.cause)
    }
}

pub trait Fetch<'a> {
    type Refs;

    fn fetch(resources: &'a Resources) -> Result<Self::Refs, CantFetch>;
}

impl<'a, R> Fetch<'a> for &'_ R
where
    R: Resource,
{
    type Refs = Ref<'a, R>;

    fn fetch(resources: &'a Resources) -> Result<Self::Refs, CantFetch> {
        resources.get().map_err(|error| CantFetch {
            type_name: type_name::<R>(),
            cause: error,
        })
    }
}

impl<'a, R> Fetch<'a> for &'_ mut R
where
    R: Resource,
{
    type Refs = RefMut<'a, R>;

    fn fetch(resources: &'a Resources) -> Result<Self::Refs, CantFetch> {
        resources.get_mut().map_err(|error| CantFetch {
            type_name: type_name::<R>(),
            cause: error,
        })
    }
}

macro_rules! expand {
    ($macro:ident, $letter:ident) => {
        $macro!($letter);
    };
    ($macro:ident, $letter:ident, $($tail:ident),*) => {
        $macro!($letter, $($tail),*);
        expand!($macro, $($tail),*);
    };
}

macro_rules! impl_for_tuples {
    ($macro:ident) => {
        expand!($macro, O, N, M, L, K, J, I, H, G, F, E, D, C, B, A);
    };
}

macro_rules! impl_fetch {
    ($($letter:ident),*) => {
        impl<'a, $($letter),*> Fetch<'a> for ($($letter,)*)
        where
            $($letter: Fetch<'a>,)*
        {
            type Refs = ($($letter::Refs,)*);

            fn fetch(resources: &'a Resources) -> Result<Self::Refs, CantFetch> {
                Ok(($($letter::fetch(resources)?,)*))
            }
        }
    }
}

impl_for_tuples!(impl_fetch);
