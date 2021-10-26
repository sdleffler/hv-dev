use core::{
    num::NonZeroUsize,
    panic::Location,
};

use slab::Slab;

#[derive(Debug)]
pub struct Lease {
    key: NonZeroUsize,
    tracker: LeaseTracker,
}

impl Lease {
    pub fn tracker(&self) -> &LeaseTracker {
        &self.tracker
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.tracker.remove_lease(self.key);
    }
}

#[derive(Debug, Clone)]
pub struct OpenLease {
    kind: Option<&'static str>,
    name: Cow<'static, str>,
}

impl OpenLease {
    pub fn kind(&self) -> Option<&str> {
        self.kind
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Debug, Clone, Default)]
pub struct LeaseTracker {
    leases: Arc<Mutex<Slab<OpenLease>>>,
}

impl LeaseTracker {
    pub fn new() -> Self {
        Default::default()
    }

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

    pub fn lease_with(&self, kind: Option<&'static str>, name: Cow<'static, str>) -> Lease {
        let mut leases = self.leases.lock().expect("lease mutex poisoned!");
        let entry = leases.vacant_entry();
        let lease = Lease {
            key: NonZeroUsize::new(entry.key() + 1).unwrap(),
            tracker: self.clone(),
        };
        entry.insert(OpenLease { kind, name });
        lease
    }

    fn remove_lease(&self, key: NonZeroUsize) {
        self.leases
            .lock()
            .expect("lease mutex poisoned!")
            .remove(key.get() - 1);
    }

    pub fn current_leases(&self) -> impl IntoIterator<Item = OpenLease> {
        self.leases
            .lock()
            .expect("lease mutex poisoned!")
            .iter()
            .map(|(_, open)| open.clone())
            .collect::<Vec<_>>()
    }
}
