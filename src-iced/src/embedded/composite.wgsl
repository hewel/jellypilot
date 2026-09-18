// iced's web-colors scene stays in extended sRGB, including video. The float
// attachment retains HDR headroom and out-of-BT.709 values through normal scene
// stacking, clipping and backdrop effects. Convert once at presentation.
@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var scene_sampler: sampler;

@vertex
fn vertex_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
  let positions = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
  return vec4<f32>(positions[index], 0.0, 1.0);
}

fn pq_encode(nits: vec3<f32>) -> vec3<f32> {
  let l = pow(max(nits, vec3<f32>(0.0)) / 10000.0, vec3<f32>(0.1593017578125));
  return pow((0.8359375 + 18.8515625 * l) / (1.0 + 18.6875 * l), vec3<f32>(78.84375));
}

fn srgb_decode(c: vec3<f32>) -> vec3<f32> {
  let magnitude = abs(c);
  let lo = magnitude / 12.92;
  let hi = pow((magnitude + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
  return sign(c) * select(hi, lo, magnitude <= vec3<f32>(0.04045));
}

const BT709_TO_BT2020 = mat3x3<f32>(
  vec3<f32>(0.6274039, 0.0690973, 0.0163914),
  vec3<f32>(0.3292830, 0.9195404, 0.0880133),
  vec3<f32>(0.0433131, 0.0113623, 0.8955953),
);

@fragment
fn hdr_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
  let rgb = textureSampleLevel(scene, scene_sampler, position.xy / vec2<f32>(textureDimensions(scene)), 0.0).rgb;
  return vec4<f32>(pq_encode(BT709_TO_BT2020 * srgb_decode(rgb) * 203.0), 1.0);
}

@fragment
fn sdr_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
  return vec4<f32>(textureSampleLevel(scene, scene_sampler, position.xy / vec2<f32>(textureDimensions(scene)), 0.0).rgb, 1.0);
}
