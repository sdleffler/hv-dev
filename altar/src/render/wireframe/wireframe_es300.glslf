in mediump vec4 v_Color;
in mediump vec3 v_Pos;
flat in mediump vec3 v_Normal;

uniform mediump float u_FogDistance;
uniform mediump vec3 u_LightDirection;
uniform mediump vec3 u_LightDiffuseColor;
uniform mediump vec3 u_LightBackColor;
uniform mediump vec3 u_LightAmbientColor;

out mediump vec4 Target0;

void main() {
    float backlit_cos_theta = dot(v_Normal, u_LightDirection);
    float diffuse_cos_theta = dot(v_Normal, -u_LightDirection);
    vec3 ambient = u_LightAmbientColor;
    vec3 backlit = u_LightBackColor * clamp(backlit_cos_theta, 0, 1);
    vec3 diffuse = u_LightDiffuseColor * clamp(diffuse_cos_theta, 0, 1);
    float dist = 1 - clamp(length(v_Pos) / u_FogDistance, 0, 1);

    Target0 = vec4(v_Color.rgb * (ambient + backlit + diffuse) * dist, v_Color.a);
}