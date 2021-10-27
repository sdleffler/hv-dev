use hv_alchemy::Type;

use crate::hv::ecs::DynamicBundleProxy;

#[cfg(feature = "hecs")]
pub mod ecs;

mod alchemy;
mod math;
mod sync;

pub trait LuaUserDataTypeExt<T> {
    /// Mark `Send + Sync` ([`Component`](hecs::Component) equivalent.)
    fn mark_component(self) -> Self
    where
        T: hecs::Component;

    /// Mark [`DynamicBundle`](hecs::DynamicBundle). (through [`DynamicBundleProxy`])
    fn mark_bundle(self) -> Self
    where
        T: hecs::DynamicBundle;
}

impl<T: 'static> LuaUserDataTypeExt<T> for Type<T> {
    fn mark_component(self) -> Self
    where
        T: hecs::Component,
    {
        self.add::<dyn Send>().add::<dyn Sync>()
    }

    fn mark_bundle(self) -> Self
    where
        T: hecs::DynamicBundle,
    {
        self.add::<dyn DynamicBundleProxy>()
    }
}
