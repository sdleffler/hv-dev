in mediump vec3 a_Pos;
in mediump vec4 a_Color;
in mediump vec3 a_Normal;

in mediump vec4 a_InstanceColor;

uniform u_InstanceTxs {
    // The max number of instances we can render at once, which matches the
    // `altar::render::wireframe::TX_BUFFER_SIZE` value.
    mediump mat4 u_Txs[1024];
};

uniform mediump mat4 u_View;
uniform mediump mat4 u_MVP;

out mediump vec4 v_Color;
flat out mediump vec3 v_Normal;
out mediump vec3 v_Pos;

void main() {
    mat4 tx = u_Txs[gl_InstanceID];
    v_Color = a_InstanceColor * a_Color;
    vec4 model_pos = tx * vec4(a_Pos, 1.0);
    gl_Position = u_MVP * model_pos;
    v_Normal = (tx * vec4(a_Normal, 0.0)).xyz;
    v_Pos = (u_View * model_pos).xyz;
}