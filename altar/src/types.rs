use hv::prelude::*;

pub type Float = f32;

#[derive(Debug, Clone, Copy)]
pub struct GlobalTick(pub u64);

#[derive(Debug, Clone, Copy)]
pub struct GlobalDt(pub f32);

#[derive(Debug, Clone, Copy)]
pub struct UpdateDt(pub f32);

#[derive(Debug, Clone, Copy)]
pub struct RemainingUpdateDt(pub f32);

#[derive(Debug, Clone, Copy)]
pub struct UpdateTick(pub u64);

#[derive(Debug, Clone, Copy)]
pub struct PreTickHook;

#[derive(Debug, Clone, Copy)]
pub struct UpdateHook;

#[derive(Debug, Clone, Copy)]
pub struct DrawHook;

#[derive(Debug, Clone, Copy)]
pub struct PostTickHook;

impl LuaUserData for PreTickHook {
    fn on_metatable_init(table: Type<Self>) {
        table.mark_component().add_clone().add_copy();
    }

    fn on_type_metatable_init(table: Type<Type<Self>>) {
        table.mark_component_type();
    }

    fn add_type_methods<'lua, M: LuaUserDataMethods<'lua, Type<Self>>>(methods: &mut M) {
        methods.add_function("new", |_, ()| Ok(Self));
    }
}

impl LuaUserData for UpdateHook {
    fn on_metatable_init(table: Type<Self>) {
        table.mark_component().add_clone().add_copy();
    }

    fn on_type_metatable_init(table: Type<Type<Self>>) {
        table.mark_component_type();
    }

    fn add_type_methods<'lua, M: LuaUserDataMethods<'lua, Type<Self>>>(methods: &mut M) {
        methods.add_function("new", |_, ()| Ok(Self));
    }
}

impl LuaUserData for DrawHook {
    fn on_metatable_init(table: Type<Self>) {
        table.mark_component().add_clone().add_copy();
    }

    fn on_type_metatable_init(table: Type<Type<Self>>) {
        table.mark_component_type();
    }

    fn add_type_methods<'lua, M: LuaUserDataMethods<'lua, Type<Self>>>(methods: &mut M) {
        methods.add_function("new", |_, ()| Ok(Self));
    }
}

impl LuaUserData for PostTickHook {
    fn on_metatable_init(table: Type<Self>) {
        table.mark_component().add_clone().add_copy();
    }

    fn on_type_metatable_init(table: Type<Type<Self>>) {
        table.mark_component_type();
    }

    fn add_type_methods<'lua, M: LuaUserDataMethods<'lua, Type<Self>>>(methods: &mut M) {
        methods.add_function("new", |_, ()| Ok(Self));
    }
}
