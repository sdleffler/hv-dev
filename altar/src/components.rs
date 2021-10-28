use hv::{
    math::{Point3, Vector3, Velocity2},
    prelude::*,
};

use crate::Float;

#[derive(Debug, Clone, Copy)]
pub struct Position {
    pub value: Point3<Float>,
}

impl Position {
    pub fn new(x: Float, y: Float, z: Float) -> Self {
        Self {
            value: Point3::new(x, y, z),
        }
    }
}

impl From<Point3<Float>> for Position {
    fn from(value: Point3<Float>) -> Self {
        Self { value }
    }
}

impl From<Vector3<Float>> for Position {
    fn from(value: Vector3<Float>) -> Self {
        Self::from(Point3::from(value))
    }
}

impl LuaUserData for Position {
    fn on_metatable_init(table: Type<Self>) {
        table
            .add_clone()
            .add_copy()
            .mark_component()
            .add_conversion_from::<Point3<Float>>()
            .add_conversion_from::<Vector3<Float>>();
    }

    #[allow(clippy::unit_arg)]
    fn add_fields<'lua, F: LuaUserDataFields<'lua, Self>>(fields: &mut F) {
        macro_rules! coords {
            ($($a:tt),*) => {{$(
                fields.add_field_method_get(stringify!($a), |_, this| Ok(this.value.$a));
                fields.add_field_method_set(stringify!($a), |_, this, a| Ok(this.value.$a = a));
            )*}}
        }

        coords!(x, y, z);
    }

    fn on_type_metatable_init(table: Type<Type<Self>>) {
        table.mark_component_type();
    }

    fn add_type_methods<'lua, M: LuaUserDataMethods<'lua, Type<Self>>>(methods: &mut M)
    where
        Self: 'static,
    {
        methods.add_function("new", |_, (x, y, z)| Ok(Self::new(x, y, z)));
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Velocity {
    pub xy: Velocity2<Float>,
    pub z: Float,
}

impl Velocity {
    pub fn new(xy: Velocity2<Float>, z: Float) -> Self {
        Self { xy, z }
    }
}

impl LuaUserData for Velocity {
    fn on_metatable_init(table: Type<Self>) {
        table.add_clone().add_copy().mark_component();
    }

    fn on_type_metatable_init(table: Type<Type<Self>>) {
        table.mark_component_type();
    }

    fn add_type_methods<'lua, M: LuaUserDataMethods<'lua, Type<Self>>>(methods: &mut M)
    where
        Self: 'static,
    {
        methods.add_function("new", |_, (v2, z)| Ok(Self::new(v2, z)));
    }
}
