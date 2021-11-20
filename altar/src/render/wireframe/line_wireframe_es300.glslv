// Based on https://stackoverflow.com/a/59688394
//
// Originally SSBO-based line rendering - builds a strip of screen-aligned quads out of triangles,
// from an attributeless rendering scheme. Modified to use UBOs.

uniform u_Positions
{
    mediump vec4 a_Position[1024];
};

uniform u_Colors
{
    mediump vec4 a_Color[1024];
};

uniform u_Indices
{
    // in Rust, this is a u32, for Reasons. It should be fine for it to be interpreted as int here,
    // since we would use a short if we could - should never get large enough to be a problem.
    int a_Index[1024];
};

uniform mediump mat4 u_MVP;
uniform mediump mat4 u_View;
uniform mediump vec2 u_Resolution;
uniform mediump float u_Thickness;

out mediump vec3 v_ViewPos;
out mediump vec4 v_Color;

void main()
{
    int line_i = a_Index[gl_VertexID / 6];
    int tri_i  = gl_VertexID % 6;

    vec4 va[4];
    for (int i=0; i<4; ++i)
    {
        va[i] = u_MVP * a_Position[line_i+i];
        va[i].xy /= va[i].w;
        va[i].xy = (va[i].xy + 1.0) * 0.5 * u_Resolution;
    }

    vec2 v_line  = normalize(va[2].xy - va[1].xy);
    vec2 nv_line = vec2(-v_line.y, v_line.x);

    vec4 pos;
    if (tri_i == 0 || tri_i == 1 || tri_i == 3)
    {
        vec2 v_pred = va[1].xy - va[0].xy;
        float pred_l = length(v_pred);
        v_pred = pred_l > 0.1 ? v_pred / pred_l : vec2(0);
        vec2 v_miter = normalize(nv_line + vec2(-v_pred.y, v_pred.x));

        pos = va[1];
        pos.xy += v_miter * u_Thickness * (tri_i == 1 ? -0.5 : 0.5) / (dot(v_miter, nv_line) * pos.w);
        v_Color = a_Color[line_i+1];
        v_ViewPos = (u_View * a_Position[line_i+1]).xyz;
    }
    else
    {
        vec2 v_succ = va[3].xy - va[2].xy;
        float succ_l = length(v_succ);
        v_succ = succ_l > 0.1 ? v_succ / succ_l : vec2(0);
        vec2 v_miter = normalize(nv_line + vec2(-v_succ.y, v_succ.x));

        pos = va[2];
        pos.xy += v_miter * u_Thickness * (tri_i == 5 ? 0.5 : -0.5) / (dot(v_miter, nv_line) * pos.w);
        v_Color = a_Color[line_i+2];
        v_ViewPos = (u_View * a_Position[line_i+2]).xyz;
    }

    pos.xy = pos.xy / u_Resolution * 2.0 - 1.0;
    pos.xy *= pos.w;
    gl_Position = pos;
}