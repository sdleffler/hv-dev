use std::sync::Arc;

use hv_alchemy::Type;
use hv_math::{Isometry3, Point3};
use parry3d::{
    math::Real,
    query::Contact,
    shape::{Ball, Capsule, Compound, ConvexPolyhedron, Cuboid, Shape, SharedShape},
};

use crate::{
    from_table::Sequence,
    hv::{LuaUserDataTypeExt, LuaUserDataTypeTypeExt},
    AnyUserData, Error, ExternalResult, Lua, Result, Table, ToLua, UserData, UserDataFields,
    UserDataMethods, Value,
};

trait LuaShapeTypeExt<T> {
    fn mark_shape(self) -> Self
    where
        T: Shape;
}

impl<T: 'static + UserData> LuaShapeTypeExt<T> for Type<T> {
    fn mark_shape(self) -> Self
    where
        T: Shape,
    {
        self.add::<dyn Shape>()
    }
}

trait LuaShapeTypeTypeExt<T> {}

impl<T> LuaShapeTypeTypeExt<T> for Type<Type<T>> {}

impl UserData for Ball {
    fn on_metatable_init(table: Type<Self>) {
        table.add_clone().add_copy().mark_component().mark_shape();
    }

    fn on_type_metatable_init(table: Type<Type<Self>>) {
        table.mark_component_type();
    }

    fn add_type_methods<'lua, M: UserDataMethods<'lua, Type<Self>>>(methods: &mut M) {
        methods.add_function("new", |_, radius| Ok(Self::new(radius)));
    }
}

impl UserData for Capsule {
    fn on_metatable_init(table: Type<Self>) {
        table.add_clone().add_copy().mark_component().mark_shape();
    }

    fn on_type_metatable_init(table: Type<Type<Self>>) {
        table.mark_component_type();
    }

    fn add_type_methods<'lua, M: UserDataMethods<'lua, Type<Self>>>(methods: &mut M) {
        methods.add_function("new", |_, (a, b, radius)| Ok(Self::new(a, b, radius)));
    }
}

impl UserData for Compound {
    fn on_metatable_init(table: Type<Self>) {
        table.add_clone().mark_component().mark_shape();
    }

    fn on_type_metatable_init(table: Type<Type<Self>>) {
        table.mark_component_type();
    }

    fn add_type_methods<'lua, M: UserDataMethods<'lua, Type<Self>>>(methods: &mut M) {
        methods.add_function(
            "new",
            |_, (txs, shapes): (Sequence<Isometry3<Real>>, Sequence<SharedShape>)| {
                let pairs = txs.into_iter().zip(shapes).collect();
                Ok(Self::new(pairs))
            },
        );
    }
}

impl UserData for ConvexPolyhedron {
    fn on_metatable_init(table: Type<Self>) {
        table.add_clone().mark_component().mark_shape();
    }

    fn on_type_metatable_init(table: Type<Type<Self>>) {
        table.mark_component_type();
    }

    fn add_type_methods<'lua, M: UserDataMethods<'lua, Type<Self>>>(methods: &mut M) {
        methods.add_function("from_convex_hull", |_, points: Sequence<Point3<Real>>| {
            Ok(Self::from_convex_hull(&points))
        });

        methods.add_function(
            "from_convex_mesh",
            |_, (points, indices): (Sequence<Point3<Real>>, Sequence<[u32; 3]>)| {
                Ok(Self::from_convex_mesh(points.0, &indices))
            },
        );
    }
}

impl UserData for Cuboid {
    fn on_metatable_init(table: Type<Self>) {
        table.add_clone().add_copy().mark_component().mark_shape();
    }

    fn on_type_metatable_init(table: Type<Type<Self>>) {
        table.mark_component_type();
    }

    fn add_type_methods<'lua, M: UserDataMethods<'lua, Type<Self>>>(methods: &mut M) {
        methods.add_function("new", |_, half_extents| Ok(Self::new(half_extents)));
    }
}

impl UserData for SharedShape {
    fn on_metatable_init(table: Type<Self>) {
        table.add_clone().mark_component();
    }

    fn on_type_metatable_init(table: Type<Type<Self>>) {
        table.mark_component_type();
    }

    fn add_type_methods<'lua, M: UserDataMethods<'lua, Type<Self>>>(methods: &mut M) {
        methods.add_function("new", |_, ud: AnyUserData| {
            Ok(Self(Arc::from(ud.dyn_clone_or_take::<dyn Shape>()?)))
        });

        methods.add_function("ball", |_, radius| Ok(Self::ball(radius)));
        methods.add_function("capsule", |_, (a, b, radius)| {
            Ok(Self::capsule(a, b, radius))
        });
        methods.add_function(
            "compound",
            |_, (txs, shapes): (Sequence<Isometry3<Real>>, Sequence<SharedShape>)| {
                let pairs = txs.into_iter().zip(shapes).collect();
                Ok(Self::compound(pairs))
            },
        );
        methods.add_function("convex_hull", |_, points: Sequence<Point3<Real>>| {
            Ok(Self::convex_hull(&points))
        });
        methods.add_function(
            "convex_mesh",
            |_, (points, indices): (Sequence<_>, Sequence<[u32; 3]>)| {
                Ok(Self::convex_mesh(points.0, &indices.0))
            },
        );
        methods.add_function("cuboid", |_, (hx, hy, hz)| Ok(Self::cuboid(hx, hy, hz)));
    }
}

impl UserData for Contact {
    fn on_metatable_init(table: Type<Self>) {
        table.add_clone().add_copy();
    }

    fn add_fields<'lua, F: UserDataFields<'lua, Self>>(fields: &mut F) {
        fields.add_field_method_get("point1", |_, this| Ok(this.point1));
        fields.add_field_method_get("point2", |_, this| Ok(this.point2));
        fields.add_field_method_get("normal1", |_, this| Ok(this.normal1));
        fields.add_field_method_get("normal2", |_, this| Ok(this.normal2));
        fields.add_field_method_get("dist", |_, this| Ok(this.dist));
    }
}

pub struct Module;

fn double_dyn_shape<R>(
    ud1: &AnyUserData,
    ud2: &AnyUserData,
    f: impl FnOnce(Option<&dyn Shape>, Option<&dyn Shape>) -> R,
) -> R {
    let (shape1, shared1);
    let (shape2, shared2);

    let s1 = if let Ok(shared) = ud1.borrow::<SharedShape>() {
        shared1 = shared;
        Some(&**shared1)
    } else if let Ok(dyn_shape) = ud1.dyn_borrow::<dyn Shape>() {
        shape1 = dyn_shape;
        Some(&*shape1)
    } else {
        None
    };

    let s2 = if let Ok(shared) = ud2.borrow::<SharedShape>() {
        shared2 = shared;
        Some(&**shared2)
    } else if let Ok(dyn_shape) = ud2.dyn_borrow::<dyn Shape>() {
        shape2 = dyn_shape;
        Some(&*shape2)
    } else {
        None
    };

    f(s1, s2)
}

impl Module {
    pub fn query_contact<'lua>(
        _lua: &'lua Lua,
        (pos1, g1, pos2, g2, prediction): (
            Isometry3<Real>,
            AnyUserData<'lua>,
            Isometry3<Real>,
            AnyUserData<'lua>,
            f32,
        ),
    ) -> Result<Option<Contact>> {
        double_dyn_shape(&g1, &g2, |maybe_g1, maybe_g2| {
            let g1 = maybe_g1.ok_or_else(|| Error::FromLuaConversionError {
                from: "userdata",
                to: "dyn Shape or ShapeHandle",
                message: Some(
                    "expected either a registered `dyn Shape` or `ShapeHandle` for argument #2!"
                        .to_owned(),
                ),
            })?;

            let g2 = maybe_g2.ok_or_else(|| Error::FromLuaConversionError {
                from: "userdata",
                to: "dyn Shape or ShapeHandle",
                message: Some(
                    "expected either a registered `dyn Shape` or `ShapeHandle` for argument #4!"
                        .to_owned(),
                ),
            })?;

            parry3d::query::contact(&pos1, g1, &pos2, g2, prediction).to_lua_err()
        })
    }

    pub fn query_distance<'lua>(
        _lua: &'lua Lua,
        (pos1, g1, pos2, g2): (
            Isometry3<Real>,
            AnyUserData<'lua>,
            Isometry3<Real>,
            AnyUserData<'lua>,
        ),
    ) -> Result<Real> {
        double_dyn_shape(&g1, &g2, |maybe_g1, maybe_g2| {
            let g1 = maybe_g1.ok_or_else(|| Error::FromLuaConversionError {
                from: "userdata",
                to: "dyn Shape or ShapeHandle",
                message: Some(
                    "expected either a registered `dyn Shape` or `ShapeHandle` for argument #2!"
                        .to_owned(),
                ),
            })?;

            let g2 = maybe_g2.ok_or_else(|| Error::FromLuaConversionError {
                from: "userdata",
                to: "dyn Shape or ShapeHandle",
                message: Some(
                    "expected either a registered `dyn Shape` or `ShapeHandle` for argument #4!"
                        .to_owned(),
                ),
            })?;

            parry3d::query::distance(&pos1, g1, &pos2, g2).to_lua_err()
        })
    }

    pub fn query_intersection_test<'lua>(
        _lua: &'lua Lua,
        (pos1, g1, pos2, g2): (
            Isometry3<Real>,
            AnyUserData<'lua>,
            Isometry3<Real>,
            AnyUserData<'lua>,
        ),
    ) -> Result<bool> {
        double_dyn_shape(&g1, &g2, |maybe_g1, maybe_g2| {
            let g1 = maybe_g1.ok_or_else(|| Error::FromLuaConversionError {
                from: "userdata",
                to: "dyn Shape or ShapeHandle",
                message: Some(
                    "expected either a registered `dyn Shape` or `ShapeHandle` for argument #2!"
                        .to_owned(),
                ),
            })?;

            let g2 = maybe_g2.ok_or_else(|| Error::FromLuaConversionError {
                from: "userdata",
                to: "dyn Shape or ShapeHandle",
                message: Some(
                    "expected either a registered `dyn Shape` or `ShapeHandle` for argument #4!"
                        .to_owned(),
                ),
            })?;

            parry3d::query::intersection_test(&pos1, g1, &pos2, g2).to_lua_err()
        })
    }
}

impl<'lua> ToLua<'lua> for Module {
    fn to_lua(self, lua: &'lua Lua) -> Result<Value<'lua>> {
        table(lua).map(Value::Table)
    }
}

pub fn table(lua: &Lua) -> Result<Table> {
    use Module as LF;

    #[rustfmt::skip]
    let shape = vec![
        ("Ball", lua.create_userdata_type::<Ball>()?),
        ("Capsule", lua.create_userdata_type::<Capsule>()?),
        ("Compound", lua.create_userdata_type::<Compound>()?),
        ("ConvexPolyhedron", lua.create_userdata_type::<ConvexPolyhedron>()?),
        ("Cuboid", lua.create_userdata_type::<Cuboid>()?),
        ("SharedShape", lua.create_userdata_type::<SharedShape>()?),
    ];

    #[rustfmt::skip]
    let query = vec![
        ("contact", lua.create_function(LF::query_contact)?),
        ("distance", lua.create_function(LF::query_distance)?),
        ("intersection_test", lua.create_function(LF::query_intersection_test)?),
    ];

    let module = vec![
        ("shape", lua.create_table_from(shape)?),
        ("query", lua.create_table_from(query)?),
    ];

    lua.create_table_from(module)
}
