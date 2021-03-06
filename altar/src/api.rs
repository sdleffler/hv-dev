use hv::{
    prelude::*,
    script::api::{Module, ModuleBuilder},
};
use parry3d::shape::SharedShape;

lazy_static::lazy_static! {
    pub static ref ALTAR: Module = Module::new("altar", "altar", altar_module);
    pub static ref PHYSICS: Module = Module::new("physics", "altar.physics", physics_module);
    pub static ref TYPES: Module = Module::new("types", "altar.types", types_module);
}

fn altar_module(lua: &Lua) -> Result<ModuleBuilder> {
    let mut builder = ModuleBuilder::new(lua)?;
    builder.submodule(&*PHYSICS)?.submodule(&*TYPES)?;

    Ok(builder)
}

fn physics_module(lua: &Lua) -> Result<ModuleBuilder> {
    use crate::physics::*;
    let mut builder = ModuleBuilder::new(lua)?;
    builder
        .userdata_type::<Position>("Position")?
        .userdata_type::<Velocity>("Velocity")?
        .userdata_type::<CompositePosition3>("CompositePosition3")?
        .userdata_type::<CompositeVelocity3>("CompositeVelocity3")?
        .userdata_type::<Physics>("Physics")?
        .userdata_type::<CcdEnabled>("CcdEnabled")?
        .userdata_type::<KinematicMarker>("KinematicMarker")?
        .userdata_type::<SharedShape>("Shape")?;

    Ok(builder)
}

fn types_module(lua: &Lua) -> Result<ModuleBuilder> {
    use crate::types::*;
    let mut builder = ModuleBuilder::new(lua)?;
    builder
        .userdata_type::<PreTickHook>("PreTickHook")?
        .userdata_type::<UpdateHook>("UpdateHook")?
        .userdata_type::<DrawHook>("DrawHook")?
        .userdata_type::<PostTickHook>("PostTickHook")?;

    Ok(builder)
}

pub fn create_lua_context() -> Result<Lua> {
    Ok(Lua::new())
}
