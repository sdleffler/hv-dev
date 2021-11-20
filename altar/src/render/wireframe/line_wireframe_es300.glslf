in mediump vec3 v_ViewPos;
in mediump vec4 v_Color;

uniform mediump float u_FogDistance;

out mediump vec4 Target0;

void main() {
    float dist = 1 - clamp(length(v_ViewPos) / u_FogDistance, 0, 1);
    Target0 = vec4(v_Color.rgb * dist, v_Color.a);
}