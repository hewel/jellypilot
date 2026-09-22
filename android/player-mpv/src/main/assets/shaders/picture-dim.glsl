//!PARAM gain
//!TYPE DYNAMIC float
//!MINIMUM 0.2
//!MAXIMUM 1.0
1.0

//!HOOK OUTPUT
//!BIND HOOKED
//!DESC JellyPilot picture dim
vec4 hook() {
    vec4 color = HOOKED_tex(HOOKED_pos);
    color.rgb *= gain;
    return color;
}
