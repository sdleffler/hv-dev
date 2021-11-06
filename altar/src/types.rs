use hv::{prelude::*, sync::NoSharedAccess};

pub type Float = f32;

#[derive(Debug, Clone, Copy)]
pub struct Dt(pub f32);

#[derive(Debug, Clone, Copy)]
pub struct RemainingDt(pub f32);

pub type LuaResource = NoSharedAccess<Lua>;
