use hv_alchemy::{Type, TypeTable};

use crate::{
    Error, FromLua, LightUserData, Lua, Result, ToLua, UserData, UserDataFields, UserDataMethods,
    Value,
};

impl<'lua> ToLua<'lua> for &'static TypeTable {
    #[inline]
    fn to_lua(self, _lua: &'lua Lua) -> Result<Value<'lua>> {
        Ok(Value::LightUserData(LightUserData(
            TypeTable::to_ptr(self) as *const _ as *mut _,
        )))
    }
}

impl<'lua> FromLua<'lua> for &'static TypeTable {
    #[inline]
    fn from_lua(lua_value: Value<'lua>, lua: &'lua Lua) -> Result<Self> {
        LightUserData::from_lua(lua_value, lua).and_then(|lud| {
            TypeTable::from_ptr(lud.0 as *const _ as *const _)
                .ok_or_else(|| Error::external("invalid TypeTable pointer!"))
        })
    }
}

impl<T: 'static + UserData> UserData for Type<T> {
    fn on_metatable_init(t: Type<Self>) {
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
