use hv::prelude::*;

pub mod brisk;
pub mod color;
pub mod evol;
pub mod gui;
pub mod pipeline;
pub mod terracotta;
pub mod wireframe;

pub use color::{Color, LinearColor};
use luminance::texture::{MagFilter, MinFilter, Sampler};

pub struct Transform {
    pub tx: Matrix4<f32>,
}

#[derive(Debug, Clone, Copy)]
pub struct F32Box2 {
    origin: Point2<f32>,
    extents: Vector2<f32>,
}

impl Default for F32Box2 {
    fn default() -> Self {
        Self {
            origin: Point2::new(0., 0.),
            extents: Vector2::new(1., 1.),
        }
    }
}

impl F32Box2 {
    fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            origin: Point2::new(x, y),
            extents: Vector2::new(width, height),
        }
    }

    fn is_valid(&self) -> bool {
        self.extents.x > 0. && self.extents.y > 0.
    }

    // Bottom left, bottom right, top left, top right
    #[inline]
    fn corners(&self) -> ([f32; 2], [f32; 2], [f32; 2], [f32; 2]) {
        (
            [self.origin.x, self.origin.y],
            [self.origin.x + self.extents.x, self.origin.y],
            [self.origin.x, self.origin.y + self.extents.y],
            [
                self.origin.x + self.extents.x,
                self.origin.y + self.extents.y,
            ],
        )
    }

    fn inverse_scale(&self, width: f32, height: f32) -> Self {
        F32Box2::new(
            self.origin.x / width,
            self.origin.y / height,
            self.extents.x / width,
            self.extents.y / height,
        )
    }
}

pub fn parse_spritesheet<F: Fn(F32Box2) -> F32Box2>(// rgba_img: &RgbaImage,
    // sprite_height: u32,
    // sprite_width: u32,
    // margin: u32,
    // spacing: u32,
    // normalize_fn: F,
) -> Vec<F32Box2> {
    unimplemented!()
}

pub fn nearest_sampler() -> Sampler {
    Sampler {
        min_filter: MinFilter::Nearest,
        mag_filter: MagFilter::Nearest,
        ..Sampler::default()
    }
}
