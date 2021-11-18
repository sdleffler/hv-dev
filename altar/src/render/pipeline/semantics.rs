use hv::prelude::*;
use luminance::Semantics;

use crate::render::{Color, LinearColor};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Semantics)]
pub enum VertexSemantics {
    #[sem(name = "a_Pos", repr = "[f32; 3]", wrapper = "VertexPosition")]
    Position,
    #[sem(name = "a_Pos2", repr = "[f32; 2]", wrapper = "VertexPosition2")]
    Position2,
    #[sem(name = "a_Uv", repr = "[f32; 2]", wrapper = "VertexUv")]
    Uv,
    #[sem(name = "a_VertColor", repr = "[f32; 4]", wrapper = "VertexColor")]
    Color,
    #[sem(name = "a_Normal", repr = "[f32; 3]", wrapper = "VertexNormal")]
    Normal,
    #[sem(name = "a_Src", repr = "[f32; 4]", wrapper = "InstanceSource")]
    InstanceSource,
    #[sem(name = "a_Color", repr = "[f32; 4]", wrapper = "InstanceColor")]
    InstanceColor,
}

impl From<Vector3<f32>> for VertexPosition {
    fn from(v: Vector3<f32>) -> Self {
        Self::new(v.into())
    }
}

impl From<VertexPosition> for Vector3<f32> {
    fn from(pos: VertexPosition) -> Self {
        (*pos).into()
    }
}

impl From<Vector2<f32>> for VertexPosition2 {
    fn from(v: Vector2<f32>) -> Self {
        Self::new(v.into())
    }
}

impl From<VertexPosition2> for Vector2<f32> {
    fn from(pos: VertexPosition2) -> Self {
        (*pos).into()
    }
}

impl From<Vector2<f32>> for VertexUv {
    fn from(v: Vector2<f32>) -> Self {
        Self::new(v.into())
    }
}

impl From<Vector3<f32>> for VertexNormal {
    fn from(v: Vector3<f32>) -> Self {
        Self::new(v.into())
    }
}

impl From<VertexNormal> for Vector3<f32> {
    fn from(normal: VertexNormal) -> Self {
        (*normal).into()
    }
}

impl From<Vector4<f32>> for VertexColor {
    fn from(v: Vector4<f32>) -> Self {
        Self::new(v.into())
    }
}

impl From<LinearColor> for VertexColor {
    fn from(c: LinearColor) -> Self {
        Self::new(c.into())
    }
}

impl From<Color> for VertexColor {
    fn from(c: Color) -> Self {
        Self::new(LinearColor::from(c).into())
    }
}
