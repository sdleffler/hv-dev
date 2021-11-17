use luminance::Semantics;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Semantics)]
pub enum VertexSemantics {
    #[sem(name = "a_Pos", repr = "[f32; 3]", wrapper = "VertexPosition")]
    Position,
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
