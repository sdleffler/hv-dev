// Copyright 2021 Shea 'Decibel' Leffler and Heavy Sol

pub extern crate alchemy;
pub extern crate anyhow as error;

pub mod ecs;

pub extern crate fs;

pub mod lua {
    pub use lua::hv::*;
    pub use lua::*;
}

pub extern crate input;
pub extern crate math;
pub extern crate resources;
pub extern crate stampede as bump;
pub extern crate sync;
pub extern crate timer;

pub mod plugin;

pub mod prelude {
    pub use crate::alchemy::Type;
    pub use crate::error::*;
    pub use crate::lua::{
        chunk,
        hv::{LuaUserDataTypeExt, LuaUserDataTypeTypeExt},
        prelude::*,
    };
    pub use crate::math::*;
}
