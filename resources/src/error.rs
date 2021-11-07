use std::{
    error::Error,
    fmt::{Display, Formatter, Result as FmtResult},
};

/// Error indicating that no [`Resource`] of requested type is present in a [`Resources`] container.
///
/// [`Resource`]: trait.Resource.html
/// [`Resources`]: struct.Resources.html
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NoSuchResource;

impl Display for NoSuchResource {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.pad("no such resource")
    }
}

impl Error for NoSuchResource {}

/// Error indicating that accessing the requested [`Resource`] in a [`Resources`] container
/// via [`get`] or [`get_mut`] methods would violate borrow rules.
///
/// [`Resource`]: trait.Resource.html
/// [`Resources`]: struct.Resources.html
/// [`get`]: struct.Resources.html#method.get
/// [`get_mut`]: struct.Resources.html#method.get_mut
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum InvalidBorrow {
    /// Can't access mutably because the resource is accessed either immutably or mutably elsewhere.
    Mutable,
    /// Can't access immutably because the resource is accessed mutably elsewhere.
    Immutable,
}

impl Display for InvalidBorrow {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.pad(match self {
            InvalidBorrow::Mutable => "cannot borrow mutably",
            InvalidBorrow::Immutable => "cannot borrow immutably",
        })
    }
}

impl Error for InvalidBorrow {}

/// Errors that may occur when accessing a [`Resource`] in a [`Resources`] container
/// via [`get`] or [`get_mut`] methods.
///
/// [`Resource`]: trait.Resource.html
/// [`Resources`]: struct.Resources.html
/// [`get`]: struct.Resources.html#method.get
/// [`get_mut`]: struct.Resources.html#method.get_mut
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum CantGetResource {
    /// Accessing the resource would violate borrow rules.
    InvalidBorrow(InvalidBorrow),
    /// No resource of this type is present in the container.
    NoSuchResource(NoSuchResource),
}

impl Display for CantGetResource {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        use CantGetResource::*;
        match self {
            InvalidBorrow(error) => error.fmt(f),
            NoSuchResource(error) => error.fmt(f),
        }
    }
}

impl Error for CantGetResource {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        use CantGetResource::*;
        match self {
            InvalidBorrow(error) => Some(error),
            NoSuchResource(error) => Some(error),
        }
    }
}

impl From<NoSuchResource> for CantGetResource {
    fn from(error: NoSuchResource) -> Self {
        CantGetResource::NoSuchResource(error)
    }
}

impl From<InvalidBorrow> for CantGetResource {
    fn from(error: InvalidBorrow) -> Self {
        CantGetResource::InvalidBorrow(error)
    }
}
