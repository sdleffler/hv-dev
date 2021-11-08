use hv::prelude::*;
use parry3d::shape::SharedShape;

use crate::types::Float;

#[derive(Debug, Clone, Copy)]
pub struct Position {
    pub current: Point3<Float>,
    pub previous: Point3<Float>,
}

impl Position {
    pub fn new(x: Float, y: Float, z: Float) -> Self {
        Self {
            current: Point3::new(x, y, z),
            previous: Point3::new(x, y, z),
        }
    }
}

impl From<Point3<Float>> for Position {
    fn from(value: Point3<Float>) -> Self {
        Self {
            current: value,
            previous: value,
        }
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
                fields.add_field_method_get(stringify!($a), |_, this| Ok(this.current.$a));
                fields.add_field_method_set(stringify!($a), |_, this, a| Ok(this.current.$a = a));
            )*}}
        }

        coords!(x, y, z);

        fields.add_field_method_get("current", |_, this| Ok(this.current));
        fields.add_field_method_set("current", |_, this, value| Ok(this.current = value));
        fields.add_field_method_get("previous", |_, this| Ok(this.current));
        fields.add_field_method_set("previous", |_, this, value| Ok(this.previous = value));
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

#[derive(Clone)]
pub struct Collider {
    pub local_tx: Isometry3<Float>,
    pub shape: SharedShape,
}

impl Collider {
    pub fn new(local_tx: Isometry3<Float>, shape: SharedShape) -> Self {
        Self { local_tx, shape }
    }
}

impl LuaUserData for Collider {
    fn on_metatable_init(table: Type<Self>) {
        table.add_clone().mark_component();
    }

    #[allow(clippy::unit_arg)]
    fn add_fields<'lua, F: LuaUserDataFields<'lua, Self>>(fields: &mut F) {
        fields.add_field_method_get("local_tx", |_, this| Ok(this.local_tx));
        fields.add_field_method_set("local_tx", |_, this, local_tx| Ok(this.local_tx = local_tx));
        fields.add_field_method_get("shape", |_, this| Ok(this.shape.clone()));
        fields.add_field_method_set("shape", |_, this, shape| Ok(this.shape = shape));
    }

    fn on_type_metatable_init(table: Type<Type<Self>>) {
        table.mark_component_type();
    }

    fn add_type_methods<'lua, M: LuaUserDataMethods<'lua, Type<Self>>>(methods: &mut M) {
        methods.add_function("new", |_, (local_tx, shape)| Ok(Self::new(local_tx, shape)));
    }
}
