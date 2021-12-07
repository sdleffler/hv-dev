use hv::{
    prelude::*,
    script::api::{Module, ModuleBuilder},
};

lazy_static::lazy_static! {
    pub static ref ALTAR: Module = Module::new("altar", "altar", altar_module);
    pub static ref PHYSICS: Module = Module::new("physics", "altar.physics", physics_module);
}

fn altar_module<'lua>(_lua: &'lua Lua, builder: &mut ModuleBuilder<'lua>) -> Result<()> {
    builder.submodule(&*PHYSICS)?;

    Ok(())
}

fn physics_module<'lua>(_lua: &'lua Lua, builder: &mut ModuleBuilder<'lua>) -> Result<()> {
    use crate::physics::*;

    builder
        .userdata_type::<Position>("Position")?
        .userdata_type::<Velocity>("Velocity")?
        .userdata_type::<CompositePosition3>("CompositePosition3")?
        .userdata_type::<CompositeVelocity3>("CompositeVelocity3")?
        .userdata_type::<Physics>("Physics")?;

    Ok(())
}
