uniform sampler2D u_Texture;

in mediump vec2 v_Uv;
in mediump vec4 v_Color;

out mediump vec4 Target0;

void main() {
    Target0 = v_Color * texture(u_Texture, v_Uv);
}