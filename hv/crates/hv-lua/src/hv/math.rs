use hv_alchemy::Type;
use hv_math::{Isometry2, Point2, Point3, RealField, Vector2, Vector3, Velocity2};

use crate::{
    types::MaybeSend, AnyUserData, FromLua, Lua, MetaMethod, Result, Table, ToLua, UserData,
    UserDataFields, UserDataMethods,
};

pub trait LuaRealField: RealField + Copy + for<'lua> ToLua<'lua> + for<'lua> FromLua<'lua> {}
impl<T> LuaRealField for T where
    T: RealField + Copy + for<'lua> ToLua<'lua> + for<'lua> FromLua<'lua> + Send + Sync
{
}

macro_rules! get_set_coords {
    ($fields:ident, $($a:tt),*) => {{$(
        $fields.add_field_method_get(stringify!($a), |_, this| Ok(this.$a));
        #[allow(clippy::unit_arg)]
        $fields.add_field_method_set(stringify!($a), |_, this, a| Ok(this.$a = a));
    )*}}
}

impl<T: LuaRealField> UserData for Vector2<T> {
    fn on_metatable_init(table: Type<Self>) {
        table
            .add_clone()
            .add_copy()
            .add::<dyn Send>()
            .add::<dyn Sync>();
    }

    fn add_fields<'lua, F: crate::UserDataFields<'lua, Self>>(fields: &mut F) {
        get_set_coords!(fields, x, y);
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

    fn add_type_methods<'lua, M: UserDataMethods<'lua, Type<Self>>>(methods: &mut M)
    where
        Self: 'static,
    {
        methods.add_function("new", |_, (x, y): (T, T)| Ok(Self::new(x, y)));
    }
}

impl<T: LuaRealField> UserData for Vector3<T> {
    fn on_metatable_init(table: Type<Self>) {
        table
            .add_clone()
            .add_copy()
            .add::<dyn Send>()
            .add::<dyn Sync>()
            .add_conversion_from::<Vector3<T>>();
    }

    fn add_fields<'lua, F: crate::UserDataFields<'lua, Self>>(fields: &mut F) {
        get_set_coords!(fields, x, y, z);
    }

    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut("set", |_, this, (x, y, z): (T, T, T)| {
            *this = Self::new(x, y, z);
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

    fn add_type_methods<'lua, M: UserDataMethods<'lua, Type<Self>>>(methods: &mut M)
    where
        Self: 'static,
    {
        methods.add_function("new", |_, (x, y, z): (T, T, T)| Ok(Self::new(x, y, z)));
    }
}

impl<T: LuaRealField> UserData for Point2<T> {
    fn on_metatable_init(table: Type<Self>) {
        table
            .add_clone()
            .add_copy()
            .add::<dyn Send>()
            .add::<dyn Sync>()
            .add_conversion_from::<Vector2<T>>();
    }

    fn add_fields<'lua, F: UserDataFields<'lua, Self>>(fields: &mut F) {
        get_set_coords!(fields, x, y);
    }
}

impl<T: LuaRealField> UserData for Point3<T> {
    fn on_metatable_init(table: Type<Self>) {
        table
            .add_clone()
            .add_copy()
            .add::<dyn Send>()
            .add::<dyn Sync>()
            .add_conversion_from::<Vector3<T>>();
    }

    fn add_fields<'lua, F: UserDataFields<'lua, Self>>(fields: &mut F) {
        get_set_coords!(fields, x, y, z);
    }
}

impl<T: LuaRealField> UserData for Isometry2<T> {
    fn on_metatable_init(table: Type<Self>) {
        table
            .add_clone()
            .add_copy()
            .add::<dyn Send>()
            .add::<dyn Sync>();
    }

    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(_methods: &mut M) {}

    fn add_type_methods<'lua, M: UserDataMethods<'lua, Type<Self>>>(methods: &mut M)
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

impl<T: LuaRealField> UserData for Velocity2<T> {
    fn on_metatable_init(table: Type<Self>) {
        table
            .add_clone()
            .add_copy()
            .add::<dyn Send>()
            .add::<dyn Sync>();
    }

    fn add_type_methods<'lua, M: UserDataMethods<'lua, Type<Self>>>(methods: &mut M)
    where
        Self: 'static + MaybeSend,
    {
        methods.add_function("new", |_, (linear, angular)| Ok(Self::new(linear, angular)));
    }
}

pub fn types(lua: &Lua) -> Result<Table> {
    let src = "return family[...]";

    macro_rules! e {
        ($lua:ident, $name:ident($($ty:ty),*)) => {{
            let t = $lua.create_table()?;
            $(t.set(stringify!($ty), lua.create_userdata_type::<$name<$ty>>()?)?;)*
            let env = lua.create_table_from(vec![("family", t)])?;
            let f = lua.load(src).set_environment(env)?.into_function()?;
            (stringify!($name), f)
        }};
    }

    macro_rules! types {
        ($lua:ident, $($name:ident($($field:ty),*)),* $(,)?) => { vec![$(e!($lua, $name($($field),*))),*] };
    }

    let es = types! {lua,
        Vector2(f32, f64),
        Vector3(f32, f64),

        Point2(f32, f64),
        Point3(f32, f64),

        Isometry2(f32, f64),

        Velocity2(f32, f64),
    };

    lua.create_table_from(es)
}
