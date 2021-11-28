// Copyright 2021 Shea 'Decibel' Leffler and Heavy Sol

pub extern crate alchemy;
pub extern crate anyhow;
pub extern crate atom;
pub extern crate cell;
pub extern crate console;

pub mod ecs;

pub extern crate elastic;
pub extern crate fs;
pub extern crate gui;

pub mod lua {
    pub use lua::hv::*;
    pub use lua::*;
}

pub extern crate input;
pub extern crate math;
pub extern crate resources;
pub extern crate script;
pub extern crate stampede as bump;
pub extern crate timer;

pub mod plugin;

pub mod prelude {
    pub use crate::alchemy::Type;
    pub use crate::anyhow::{anyhow, bail, ensure, Context, Error, Result};
    pub use crate::lua::{
        chunk,
        hv::{LuaUserDataTypeExt, LuaUserDataTypeTypeExt},
        prelude::*,
    };
    pub use crate::math::*;
}
