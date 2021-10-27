pub extern crate alchemy;
pub extern crate anyhow as error;

pub mod ecs;

pub extern crate lua;
pub extern crate sync;

pub mod plugin;

pub mod prelude {
    pub use crate::alchemy::Type;
    pub use crate::error::*;
    pub use crate::lua::{chunk, prelude::*};
}
