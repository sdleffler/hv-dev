use hv_alchemy::TypedMetaTable;
use hv_math::{Isometry2, RealField, Vector2};

use crate::{AnyUserData, FromLua, MetaMethod, ToLua, UserData, UserDataMethods};

pub trait LuaRealField: RealField + Copy + for<'lua> ToLua<'lua> + for<'lua> FromLua<'lua> {}
impl<T> LuaRealField for T where
    T: RealField + Copy + for<'lua> ToLua<'lua> + for<'lua> FromLua<'lua> + Send + Sync
{
}

impl<T: LuaRealField> UserData for Vector2<T> {
    fn on_metatable_init(table: TypedMetaTable<Self>) {
        table
            .mark_clone()
            .mark_copy()
            .add::<dyn Send>()
            .add::<dyn Sync>();
    }

    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut("set", |_, this, (x, y): (T, T)| {
            *this = Self::new(x, y);
            Ok(())
        });

        methods.add_function(
            "add",
            |lua, (a, b, out): (Self, Self, Option<AnyUserData>)| match out {
                Some(ud) => {
                    *ud.borrow_mut::<Self>()? = a + b;
                    Ok(ud)
                }
                None => lua.create_userdata(a + b),
            },
        );

        methods.add_function(
            "sub",
            |lua, (a, b, out): (Self, Self, Option<AnyUserData>)| match out {
                Some(ud) => {
                    *ud.borrow_mut::<Self>()? = a - b;
                    Ok(ud)
                }
                None => lua.create_userdata(a - b),
            },
        );

        methods.add_meta_function(MetaMethod::Add, |_, (a, b): (Self, Self)| Ok(a + b));
        methods.add_meta_function(MetaMethod::Sub, |_, (a, b): (Self, Self)| Ok(a - b));
    }

    fn add_type_methods<'lua, M: UserDataMethods<'lua, TypedMetaTable<Self>>>(methods: &mut M)
    where
        Self: 'static,
    {
        methods.add_function("new", |_, (x, y): (T, T)| Ok(Self::new(x, y)));
    }
}

impl<T: LuaRealField> UserData for Isometry2<T> {
    fn on_metatable_init(table: TypedMetaTable<Self>) {
        table
            .mark_clone()
            .mark_copy()
            .add::<dyn Send>()
            .add::<dyn Sync>();
    }

    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(_methods: &mut M) {}

    fn add_type_methods<'lua, M: UserDataMethods<'lua, TypedMetaTable<Self>>>(methods: &mut M)
    where
        Self: 'static,
    {
        methods.add_function("new", |_, (t, a): (Vector2<T>, T)| Ok(Self::new(t, a)));
        methods.add_function("translation", |_, (x, y): (T, T)| {
            Ok(Self::translation(x, y))
        });
        methods.add_function("rotation", |_, angle: T| Ok(Self::rotation(angle)));
    }
}
