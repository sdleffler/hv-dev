in mediump vec3 v_ViewPos;
in mediump vec4 v_Color;

uniform mediump float u_FogDistance;

out mediump vec4 Target0;

void main() {
    float d = length(v_ViewPos);
    float d_frac = d / u_FogDistance;
    float dist = pow(1 - clamp(d_frac, 0, 1), (1 + d_frac));
    Target0 = vec4(v_Color.rgb * dist, v_Color.a);
}