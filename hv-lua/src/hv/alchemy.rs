use hv_alchemy::{AlchemyTable, TypedAlchemyTable};

use crate::{
    Error, FromLua, LightUserData, Lua, Result, ToLua, UserData, UserDataFields, UserDataMethods,
    Value,
};

impl<'lua> ToLua<'lua> for &'static AlchemyTable {
    #[inline]
    fn to_lua(self, _lua: &'lua Lua) -> Result<Value<'lua>> {
        Ok(Value::LightUserData(LightUserData(
            AlchemyTable::to_ptr(self) as *const _ as *mut _,
        )))
    }
}

impl<'lua> FromLua<'lua> for &'static AlchemyTable {
    #[inline]
    fn from_lua(lua_value: Value<'lua>, lua: &'lua Lua) -> Result<Self> {
        LightUserData::from_lua(lua_value, lua).and_then(|lud| {
            AlchemyTable::from_ptr(lud.0 as *const _ as *const _)
                .ok_or_else(|| Error::external("invalid AlchemyTable pointer!"))
        })
    }
}

impl<T: 'static + UserData> UserData for TypedAlchemyTable<T> {
    fn on_metatable_init(t: TypedAlchemyTable<Self>) {
        t.mark_clone()
            .mark_copy()
            .add::<dyn Send>()
            .add::<dyn Sync>();
        T::on_type_metatable_init(hv_alchemy::of())
    }

    fn add_fields<'lua, F: UserDataFields<'lua, Self>>(fields: &mut F) {
        T::add_type_fields(fields);
    }

    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        T::add_type_methods(methods);
    }
}
