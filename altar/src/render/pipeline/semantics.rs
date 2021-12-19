use hv::prelude::*;
use luminance::Semantics;

use crate::render::{Color, LinearColor};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Semantics)]
pub enum VertexSemantics {
    #[sem(name = "a_Pos", repr = "[f32; 3]", wrapper = "VertexPosition")]
    Position,
    #[sem(name = "a_Uv", repr = "[f32; 2]", wrapper = "VertexUv")]
    Uv,
    #[sem(name = "a_Color", repr = "[f32; 4]", wrapper = "VertexColor")]
    Color,
    #[sem(name = "a_Normal", repr = "[f32; 3]", wrapper = "VertexNormal")]
    Normal,
    #[sem(name = "a_Src", repr = "[f32; 4]", wrapper = "InstanceSource")]
    InstanceSource,
    #[sem(name = "a_InstanceColor", repr = "[f32; 4]", wrapper = "InstanceColor")]
    InstanceColor,
    #[sem(name = "a_TCol1", repr = "[f32; 4]", wrapper = "InstanceTCol1")]
    InstanceTCol1,
    #[sem(name = "a_TCol2", repr = "[f32; 4]", wrapper = "InstanceTCol2")]
    InstanceTCol2,
    #[sem(name = "a_TCol3", repr = "[f32; 4]", wrapper = "InstanceTCol3")]
    InstanceTCol3,
    #[sem(name = "a_TCol4", repr = "[f32; 4]", wrapper = "InstanceTCol4")]
    InstanceTCol4,
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

#[derive(Debug, Clone, Copy)]
pub struct InstanceTColumns {
    pub tcol1: InstanceTCol1,
    pub tcol2: InstanceTCol2,
    pub tcol3: InstanceTCol3,
    pub tcol4: InstanceTCol4,
}

impl From<Matrix4<f32>> for InstanceTColumns {
    fn from(matrix: Matrix4<f32>) -> Self {
        InstanceTColumns {
            tcol1: InstanceTCol1::new([matrix.m11, matrix.m21, matrix.m31, matrix.m41]),
            tcol2: InstanceTCol2::new([matrix.m12, matrix.m22, matrix.m32, matrix.m42]),
            tcol3: InstanceTCol3::new([matrix.m13, matrix.m23, matrix.m33, matrix.m43]),
            tcol4: InstanceTCol4::new([matrix.m14, matrix.m24, matrix.m34, matrix.m44]),
        }
    }
}

impl From<InstanceTColumns> for Matrix4<f32> {
    fn from(cols: InstanceTColumns) -> Self {
        let c1 = cols.tcol1;
        let c2 = cols.tcol2;
        let c3 = cols.tcol3;
        let c4 = cols.tcol4;
        Matrix4::from_columns(&[
            Vector4::from(*c1),
            Vector4::from(*c2),
            Vector4::from(*c3),
            Vector4::from(*c4),
        ])
    }
}
