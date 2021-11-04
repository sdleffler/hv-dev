use hv_alchemy::Type;
use hv_math::Velocity2;

use crate::{
    external::nalgebra::LuaRealField, types::MaybeSend, FromLua, Lua, Result, Table, ToLua,
    UserData, UserDataMethods, Value,
};

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

pub struct Module;

impl<'lua> ToLua<'lua> for Module {
    fn to_lua(self, lua: &'lua Lua) -> Result<Value<'lua>> {
        table(lua).map(Value::Table)
    }
}

pub fn table(lua: &Lua) -> Result<Table> {
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

    let table = Table::from_lua(crate::external::nalgebra::Module.to_lua(lua)?, lua).unwrap();

    let es = types! {lua,
        Velocity2(f32, f64),
    };

    for (k, v) in es {
        table.set(k, v)?;
    }

    Ok(table)
}
