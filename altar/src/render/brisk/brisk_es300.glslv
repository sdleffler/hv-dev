// Instance attributes
in mediump vec4 i_TCol1;
in mediump vec4 i_TCol2;
in mediump vec4 i_TCol3;
in mediump vec4 i_TCol4;

in mediump vec4 i_Uvs;
in float i_Opacity;
in uvec2 i_Dims;

// Outputs
out mediump vec2 f_Uv;
flat out float f_Opacity;

// Uniforms
uniform mediump mat4 u_Projection;

// Using attributeless rendering, so these are our vertices
const vec2[4] TRIANGLE_POS = vec2[](
    vec2(0., 0.),
    vec2(0., 1.),
    vec2(1., 1.),
    vec2(1., 0.)
);

void main() {
    mat4 Model = mat4(i_TCol1, i_TCol2, i_TCol3, i_TCol4);
    f_Opacity = i_Opacity;
    gl_Position = u_Projection * Model * vec4(TRIANGLE_POS[gl_VertexID] * vec2(i_Dims), 0., 1.);
    switch (gl_VertexID) {
	case 0:
	    f_Uv = i_Uvs.xy;
	    break;
	case 1:
	    f_Uv = i_Uvs.xw;
	    break;
	case 2:
	    f_Uv = i_Uvs.zw;
	    break;
	case 3:
	    f_Uv = i_Uvs.zy;
	    break;
    }
}
