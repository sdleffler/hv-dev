use hv_alchemy::Type;

use crate::{
    hv::ecs::{ComponentType, DynamicBundleProxy},
    Lua, RegistryKey, Result, Table, ToLua, UserData,
};

#[cfg(feature = "hv-ecs")]
pub mod ecs;
pub mod math;

pub mod alchemy;
mod sync;

pub trait LuaUserDataTypeExt<T> {
    /// Mark `Send + Sync` ([`Component`](hv_ecs::Component) equivalent.)
    fn mark_component(self) -> Self
    where
        T: hv_ecs::Component;

    /// Mark [`DynamicBundle`](hv_ecs::DynamicBundle). (through [`DynamicBundleProxy`])
    fn mark_bundle(self) -> Self
    where
        T: hv_ecs::DynamicBundle;
}

pub trait LuaUserDataTypeTypeExt<T> {
    /// Mark [`Type<T>: ComponentType`](ComponentType) (allows use for constructing dynamic queries)
    fn mark_component_type(self) -> Self
    where
        T: hv_ecs::Component;
}

impl<T: 'static + UserData> LuaUserDataTypeExt<T> for Type<T> {
    fn mark_component(self) -> Self
    where
        T: hv_ecs::Component,
    {
        self.add::<dyn Send>().add::<dyn Sync>()
    }

    fn mark_bundle(self) -> Self
    where
        T: hv_ecs::DynamicBundle,
    {
        self.add::<dyn DynamicBundleProxy>()
    }
}

impl<T: 'static + UserData> LuaUserDataTypeTypeExt<T> for Type<Type<T>> {
    fn mark_component_type(self) -> Self
    where
        T: hv_ecs::Component,
    {
        self.add::<dyn ComponentType>()
    }
}

pub fn types(lua: &Lua) -> Result<Table> {
    use crate::Value::*;
    lua.create_table_from(vec![
        ("ecs", Table(self::ecs::types(lua)?)),
        ("math", self::math::Module.to_lua(lua)?),
        (
            "RegistryKey",
            UserData(lua.create_userdata_type::<RegistryKey>()?),
        ),
    ])
}
