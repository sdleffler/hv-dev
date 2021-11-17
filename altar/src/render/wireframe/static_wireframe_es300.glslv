in mediump vec3 a_Pos;
in mediump vec4 a_VertColor;
in mediump vec3 a_Normal;

uniform mediump vec4 u_Color;
uniform mediump mat4 u_Tx;
uniform mediump mat4 u_View;
uniform mediump mat4 u_MVP;

out mediump vec4 v_Color;
flat out mediump vec3 v_Normal;
out mediump vec3 v_Pos;

void main() {
    v_Color = u_Color * a_VertColor;
    gl_Position = u_MVP * vec4(a_Pos, 1.0);
    v_Normal = (u_Tx * vec4(a_Normal, 0.0)).xyz;
    v_Pos = (u_View * u_Tx * vec4(a_Pos, 1.0)).xyz;
}