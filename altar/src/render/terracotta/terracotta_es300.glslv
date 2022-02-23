uniform mat4 u_Transform;

in mediump vec2 v_Uv;
in mediump vec3 v_Pos;
in uint v_Ts;

out mediump vec2 uv;
flat out uint f_Ts;

void main() {
    uv = v_Uv;
    gl_Position = u_Transform * vec4(v_Pos, 1.);
    f_Ts = v_Ts;
}