#![feature(allocator_api)]
#![feature(control_flow_enum)]
#![feature(generic_associated_types)]
#![feature(vec_into_raw_parts)]

pub mod command_buffer;
pub mod event_loop;
pub mod graphics;
pub mod physics;
pub mod scene;
pub mod script;
pub mod types;

#[cfg(feature = "glfw")]
pub mod glfw;

#[cfg(feature = "windowed")]
pub mod window;

pub use types::Float;
