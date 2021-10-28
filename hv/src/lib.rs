pub extern crate alchemy;
pub extern crate anyhow as error;

pub mod ecs;

pub mod lua {
    pub use lua::hv::*;
    pub use lua::*;
}

pub extern crate math;
pub extern crate sync;

pub mod plugin;

pub mod prelude {
    pub use crate::alchemy::Type;
    pub use crate::error::*;
    pub use crate::lua::{
        chunk,
        hv::{LuaUserDataTypeExt, LuaUserDataTypeTypeExt},
        prelude::*,
    };
}
