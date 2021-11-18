uniform vec2 u_TargetSize;

in mediump vec2 a_Pos;
in mediump vec2 a_Uv;
in mediump vec4 a_VertColor;

out mediump vec2 v_Uv;
out mediump vec4 v_Color;

void main() {
    gl_Position = vec4(
        2.0 * a_Pos.x / u_TargetSize.x - 1.0,
        1.0 - 2.0 * a_Pos.y / u_TargetSize.y,
        0.0,
        1.0
    );
    v_Uv = a_Uv;
    v_Color = a_VertColor;
}