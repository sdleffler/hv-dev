uniform sampler2DArray u_Textures;

flat in uint f_Ts;

in mediump vec2 uv;

out vec4 color;

void main() {
    color = texture(u_Textures, vec3(uv, f_Ts));
}