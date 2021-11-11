use hv_alchemy::Type;

use crate::{types::MaybeSend, FromLua, ToLua, UserData, UserDataMethods};

impl<T: for<'lua> FromLua<'lua> + for<'lua> ToLua<'lua>> UserData for Vec<T> {
    #[allow(clippy::unit_arg)]
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut("push", |_, this, t| Ok(this.push(t)));
    }

    fn add_type_methods<'lua, M: UserDataMethods<'lua, Type<Self>>>(methods: &mut M)
    where
        Self: 'static + MaybeSend,
    {
        methods.add_function("new", |_, ()| Ok(Vec::new()));
        methods.add_function("with_capacity", |_, ()| Ok(Vec::new()));
    }
}
