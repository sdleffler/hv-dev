#![allow(clippy::needless_option_as_deref)]
#![feature(allocator_api)]
#![feature(control_flow_enum)]
#![feature(generic_associated_types)]
#![feature(vec_into_raw_parts)]
#![feature(bool_to_option)]
#![feature(slice_partition_dedup)]
#![feature(is_sorted)]
#![feature(split_array)]
#![feature(toowned_clone_into)]
#![feature(maybe_uninit_uninit_array, maybe_uninit_array_assume_init)]

pub mod api;
pub mod collision;
pub mod command_buffer;
pub mod event_loop;
pub mod lattice;
pub mod physics;
pub mod render;
pub mod scene;
pub mod types;

#[cfg(feature = "glfw")]
pub mod glfw;

#[cfg(feature = "windowed")]
pub mod window;

pub use types::Float;
