use hv::prelude::*;

pub mod color;
pub mod evol;
pub mod gui;
pub mod pipeline;
pub mod wireframe;

pub use color::{Color, LinearColor};

pub struct Transform {
    pub tx: Matrix4<f32>,
}
