in mediump vec3 v_Barycentric; // barycentric coordinate inside the triangle
in mediump vec4 v_Color;

out mediump vec4 Target0;

uniform mediump float u_LineThickness; // thickness of the rendered lines

void main()
{
    // Find the edge this fragment is closest to.
    float ClosestEdge = min(v_Barycentric.x, min(v_Barycentric.y, v_Barycentric.z)); 
    // calculate derivative (divide u_LineThickness by this to have the line width constant in
    // screen-space)
    // WTF(sleffy)
    float Width = fwidth(ClosestEdge); 
    float EdgeIntensity = 1.0 - smoothstep(u_LineThickness, u_LineThickness + Width, ClosestEdge);

    if (EdgeIntensity == 0.0) {
        discard;
    }

    Target0 = v_Color;
}