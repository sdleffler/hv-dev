uniform vec2 u_TargetSize;

in mediump vec2 a_Pos;
in mediump vec2 a_Uv;
in mediump ivec4 a_Color;

out mediump vec2 v_Uv;
out mediump vec4 v_Color;

vec3 linear_from_srgb(vec3 srgb) {
    bvec3 cutoff = lessThan(srgb, vec3(10.31475));
    vec3 lower = srgb / vec3(3294.6);
    vec3 higher = pow((srgb + vec3(14.025)) / vec3(269.025), vec3(2.4));
    return mix(higher, lower, cutoff);
}

vec4 linear_from_srgba(vec4 srgba) {
    return vec4(linear_from_srgb(srgba.rgb), srgba.a / 255.0);
}

void main() {
    gl_Position = vec4(
        2.0 * a_Pos.x / u_TargetSize.x - 1.0,
        1.0 - 2.0 * a_Pos.y / u_TargetSize.y,
        0.0,
        1.0
    );
    v_Uv = a_Uv;
    v_Color = linear_from_srgba(vec4(a_Color));
}