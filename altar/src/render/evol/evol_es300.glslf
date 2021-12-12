in mediump vec2 v_Uv;
in mediump vec4 v_Color;

out mediump vec4 Target0;

uniform mediump sampler2D u_Texture;

void main() {
    Target0 = texture(u_Texture, v_Uv) * v_Color;
}