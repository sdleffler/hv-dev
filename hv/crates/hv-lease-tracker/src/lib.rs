//! Heavy Lease Tracker - functionality for tracking borrows and providing more helpful errors on
//! runtime borrowing/aliasing violations.
//!
//! - [`LeaseTracker`] type, for use in smart cells like `hv-cell`'s `AtomicRefCell` (only on debug
//!   with `track-leases` feature enabled)
//! - [`Lease`] type, for use in smart references/guard types like `hv-cell`'s
//!   `AtomicRef`/`AtomicRefMut` and `ArcRef`/`ArcRefMut`
//! - [`OpenLease`] type, representing the origin of a dynamic borrow for diagnostic usage
//!
//! `no_std` compatible, but requires `alloc` for `Arc`s, `Cow`s, and `String`s and uses spinlocks
//! internally to [`LeaseTracker`] for synchronizing adding/removing [`OpenLease`]s.
//!
//! `hv-lease-tracker` is not a super performant crate, and should probably be disabled in your
//! release builds if performance of borrows is critical. It is strictly for diagnostic info.

#![no_std]
#![warn(missing_docs)]

use core::{num::NonZeroUsize, panic::Location};

extern crate alloc;

use alloc::{borrow::Cow, format, sync::Arc, vec::Vec};
use slab::Slab;
use spin::Mutex;

/// A handle to an [`OpenLease`]. Created from a [`LeaseTracker`], and carries an index pointing to
/// debug information about that lease inside its tracker. This type is a drop guard and should be
/// kept in your smart reference/guard type; when dropped, it removes its associated lease from its
/// tracker.
///
/// It is not clone and as such when cloning something like a shared reference guard type, you'll
/// need to re-[`Lease`] using the tracker reference from [`Lease::tracker`].
#[derive(Debug)]
pub struct Lease {
    key: NonZeroUsize,
    tracker: LeaseTracker,
}

impl Lease {
    /// Get a reference to the lease tracker this `Lease` came from. Useful for getting another
    /// lease from the same tracker.
    pub fn tracker(&self) -> &LeaseTracker {
        &self.tracker
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.tracker.remove_lease(self.key);
    }
}

/// An [`OpenLease`] represents the origin of an ongoing dynamic borrow. Stored as an entry for a
/// [`Lease`] inside a [`LeaseTracker`].
#[derive(Debug, Clone)]
pub struct OpenLease {
    kind: Option<&'static str>,
    name: Cow<'static, str>,
}

impl OpenLease {
    /// An optional human-readable string identifying the borrow kind (most likely mutable or
    /// immutable.)
    pub fn kind(&self) -> Option<&str> {
        self.kind
    }

    /// A human-readable string identifying the source of this borrow. Most of the time, created
    /// automatically from the [`Location`] API, containing the source file/line.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// A registry which tracks the origins of dynamic borrows to provide better debug information on a
/// borrow error.
#[derive(Debug, Clone, Default)]
pub struct LeaseTracker {
    leases: Arc<Mutex<Slab<OpenLease>>>,
}

impl LeaseTracker {
    /// Create an empty [`LeaseTracker`].
    pub fn new() -> Self {
        Default::default()
    }

    /// Register a lease using [`Location`] info from the caller to generate the `name` field of the
    /// [`OpenLease`].
    #[track_caller]
    pub fn lease_at_caller(&self, kind: Option<&'static str>) -> Lease {
        let location = Location::caller();
        self.lease_with(
            kind,
            Cow::Owned(format!(
                "{} (line {}, column {})",
                location.file(),
                location.line(),
                location.column()
            )),
        )
    }

    /// Register a lease using a custom name string rather than generating it from caller
    /// information.
    pub fn lease_with(&self, kind: Option<&'static str>, name: Cow<'static, str>) -> Lease {
        let mut leases = self.leases.lock();
        let entry = leases.vacant_entry();
        let lease = Lease {
            key: NonZeroUsize::new(entry.key() + 1).unwrap(),
            tracker: self.clone(),
        };
        entry.insert(OpenLease { kind, name });
        lease
    }

    fn remove_lease(&self, key: NonZeroUsize) {
        self.leases.lock().remove(key.get() - 1);
    }

    /// Iterate over all currently open leases.
    pub fn current_leases(&self) -> impl IntoIterator<Item = OpenLease> {
        self.leases
            .lock()
            .iter()
            .map(|(_, open)| open.clone())
            .collect::<Vec<_>>()
    }
}
