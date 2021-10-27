use hv_alchemy::Type;

use crate::{
    hv::ecs::{ComponentType, DynamicBundleProxy},
    UserData,
};

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

pub trait LuaUserDataTypeTypeExt<T> {
    /// Mark [`Type<T>: ComponentType`](ComponentType) (allows use for constructing dynamic queries)
    fn mark_component_type(self) -> Self
    where
        T: hecs::Component;
}

impl<T: 'static + UserData> LuaUserDataTypeExt<T> for Type<T> {
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

impl<T: 'static + UserData> LuaUserDataTypeTypeExt<T> for Type<Type<T>> {
    fn mark_component_type(self) -> Self
    where
        T: hecs::Component,
    {
        self.add::<dyn ComponentType>()
    }
}
