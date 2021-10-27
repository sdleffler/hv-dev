use std::sync::Arc;

use glfw::Context;
use hv::{
    alchemy::Type,
    ecs::{ComponentType, DynamicQuery, Entity, World},
    lua::{chunk, Lua, UserData, UserDataFields, UserDataMethods},
    sync::cell::AtomicRefCell,
};
use luminance_glfw::GlfwSurface;
use luminance_windowing::{WindowDim, WindowOpt};

#[derive(Debug, Clone, Copy)]
struct I32Component(i32);

impl UserData for I32Component {
    #[allow(clippy::unit_arg)]
    fn add_fields<'lua, F: UserDataFields<'lua, Self>>(fields: &mut F) {
        fields.add_field_method_get("value", |_, this| Ok(this.0));
        fields.add_field_method_set("value", |_, this, value| Ok(this.0 = value));
    }

    fn on_metatable_init(t: Type<Self>) {
        t.mark_clone()
            .mark_copy()
            .add::<dyn Send>()
            .add::<dyn Sync>();
    }

    fn add_type_methods<'lua, M: UserDataMethods<'lua, Type<Self>>>(methods: &mut M) {
        methods.add_function("new", |_, i: i32| Ok(Self(i)));
    }

    fn on_type_metatable_init(t: Type<Type<Self>>) {
        t.add::<dyn ComponentType>();
    }
}

#[derive(Debug, Clone, Copy)]
struct BoolComponent(bool);

impl UserData for BoolComponent {
    #[allow(clippy::unit_arg)]
    fn add_fields<'lua, F: UserDataFields<'lua, Self>>(fields: &mut F) {
        fields.add_field_method_get("value", |_, this| Ok(this.0));
        fields.add_field_method_set("value", |_, this, value| Ok(this.0 = value));
    }

    fn on_metatable_init(t: Type<Self>) {
        t.mark_clone()
            .mark_copy()
            .add::<dyn Send>()
            .add::<dyn Sync>();
    }

    fn add_type_methods<'lua, M: UserDataMethods<'lua, Type<Self>>>(methods: &mut M) {
        methods.add_function("new", |_, b: bool| Ok(Self(b)));
    }

    fn on_type_metatable_init(t: Type<Type<Self>>) {
        t.add::<dyn ComponentType>();
    }
}

fn main() -> hv::lua::Result<()> {
    let lua = Lua::new();

    let world_ty = lua.create_userdata_type::<World>()?;
    let query_ty = lua.create_userdata_type::<DynamicQuery>()?;
    let i32_ty = lua.create_userdata_type::<I32Component>()?;
    let bool_ty = lua.create_userdata_type::<BoolComponent>()?;
    let world = Arc::new(AtomicRefCell::new(World::new()));
    let world_clone = world.clone();

    let chunk = chunk! {
        local World = $world_ty
        local Query = $query_ty
        local I32 = $i32_ty
        local Bool = $bool_ty

        local world = $world_clone
        local entity = world:spawn { I32.new(5), Bool.new(true) }
        local query = Query.new { Query.write(I32), Query.read(Bool) }
        world:query_one(query, entity, function(item)
            assert(item:take(Bool).value == true)
            local i = item:take(I32)
            assert(i.value == 5)
            i.value = 6
            assert(i.value == 6)
        end)

        return entity
    };

    let entity: Entity = lua.load(chunk).eval()?;

    let borrowed = world.borrow();
    let mut q = borrowed
        .query_one::<(&I32Component, &BoolComponent)>(entity)
        .ok();
    assert_eq!(
        q.as_mut().and_then(|q| q.get()).map(|(i, b)| (i.0, b.0)),
        Some((6, true))
    );

    let dim = WindowDim::Windowed {
        width: 960,
        height: 540,
    };
    let surface = GlfwSurface::new_gl33("scratch", WindowOpt::default().set_dim(dim))
        .expect("GLFW surface creation");
    let events = surface.events_rx;
    let mut context = surface.context;

    'app: loop {
        context.window.glfw.poll_events();

        context.window.swap_buffers();

        for (t, event) in glfw::flush_messages(&events) {
            match event {
                glfw::WindowEvent::Close => break 'app,
                _ => {}
            }
        }
    }

    Ok(())
}
