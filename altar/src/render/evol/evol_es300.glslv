// Vertex attributes.
in mediump vec3 a_Pos;
in mediump vec4 a_Color;
in mediump vec2 a_Uv;

// Instance attributes.
in mediump vec4 a_Src;
in mediump vec4 a_InstanceColor;
in mediump vec4 a_TCol1;
in mediump vec4 a_TCol2;
in mediump vec4 a_TCol3;
in mediump vec4 a_TCol4;

out mediump vec2 v_Uv;
out mediump vec4 v_Color;
out mediump vec3 v_Barycentric;

uniform mediump vec2 u_TargetSize;
uniform mediump mat4 u_ViewProjection;

void main() {
    mat4 Model = mat4(a_TCol1, a_TCol2, a_TCol3, a_TCol4);
    // Project the vertex without performing the perspective divide.
    gl_Position = u_ViewProjection * Model * vec4(a_Pos, 1);
    v_Uv = a_Uv * a_Src.zw + a_Src.xy;
    v_Color = a_InstanceColor * a_Color;

    switch (gl_VertexID % 3) {
    case 0:
        v_Barycentric = vec3(1, 0, 0);
        break;
    case 1:
        v_Barycentric = vec3(0, 1, 0);
        break;
    case 2:
        v_Barycentric = vec3(0, 0, 1);
        break;
    }
}
