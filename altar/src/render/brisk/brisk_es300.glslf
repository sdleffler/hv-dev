uniform sampler2D u_Texture;

in mediump vec2 f_Uv;
flat in float f_Opacity;

out vec4 color;

void main() {
    color = texture(u_Texture, vec2(f_Uv));
    color.a *= f_Opacity;
}
