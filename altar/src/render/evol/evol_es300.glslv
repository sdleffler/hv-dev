// Vertex attributes.
in mediump vec3 a_Pos;
in mediump vec2 a_ScreenSpaceOffset;
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

uniform mediump vec2 u_TargetSize;
uniform mediump mat4 u_ViewProjection;

void main() {
    mat4 Model = mat4(a_TCol1, a_TCol2, a_TCol3, a_TCol4);
    // Project the vertex without performing the perspective divide.
    gl_Position = u_ViewProjection * Model * vec4(a_Pos, 1);
    // Add in the screen space offset, converted into NDC...
    // The perspective divide will ensure that the screen space offset is properly scaled, so lines
    // will appear to narrow with distance, etc. as should properly occur if the projection is set
    // up as a perspective projection.
    gl_Position.xy += a_ScreenSpaceOffset / u_TargetSize * 2;
    v_Uv = a_Uv * a_Src.zw + a_Src.xy;
    v_Color = a_InstanceColor * a_Color;
}
