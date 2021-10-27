#![feature(const_fn_trait_bound)]
#![feature(generic_associated_types)]
#![feature(result_into_ok_or_err)]
#![feature(slice_ptr_get, slice_ptr_len)]
#![no_std]

extern crate alloc;

pub mod atom;
pub mod borrow;
pub mod capability;
pub mod cell;
#[macro_use]
pub mod elastic;
pub mod monotonic_list;

#[cfg(feature = "track-leases")]
pub mod lease;

mod hv {
    mod ecs;
}
