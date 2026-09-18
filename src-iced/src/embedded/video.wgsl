struct VertexOutput {
  @builtin(position) position: vec4<f32>,
  @location(0) uv: vec2<f32>,
};
@group(0) @binding(0) var video: texture_2d<f32>;
@group(0) @binding(1) var video_sampler: sampler;
@vertex
fn vertex_main(@builtin(vertex_index) index: u32) -> VertexOutput {
  let positions = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
  let position = positions[index];
  var output: VertexOutput;
  output.position = vec4<f32>(position, 0.0, 1.0);
  output.uv = vec2<f32>(position.x * 0.5 + 0.5, 0.5 - position.y * 0.5);
  return output;
}
@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
  // mpv already rendered BT.709 gamma-2.2 into RGB10A2 UNORM. Preserve code
  // values, including premultiplied RGB; opaque output composites over black.
  return vec4<f32>(textureSample(video, video_sampler, input.uv).rgb, 1.0);
}

fn pq_decode(c: vec3<f32>) -> vec3<f32> {
  let p = pow(c, vec3<f32>(1.0 / 78.84375));
  let num = max(p - vec3<f32>(0.8359375), vec3<f32>(0.0));
  return 10000.0 * pow(num / (18.8515625 - 18.6875 * p), vec3<f32>(1.0 / 0.1593017578125));
}

fn srgb_encode(c: vec3<f32>) -> vec3<f32> {
  let magnitude = abs(c);
  let lo = 12.92 * magnitude;
  let hi = 1.055 * pow(magnitude, vec3<f32>(1.0 / 2.4)) - 0.055;
  return sign(c) * select(hi, lo, magnitude <= vec3<f32>(0.0031308));
}

const BT2020_TO_BT709 = mat3x3<f32>(
  vec3<f32>(1.6604910, -0.1245505, -0.0181508),
  vec3<f32>(-0.5876411, 1.1328999, -0.1005789),
  vec3<f32>(-0.0728499, -0.0083494, 1.1187297),
);

@fragment
fn hdr_main(input: VertexOutput) -> @location(0) vec4<f32> {
  let nits = pq_decode(textureSample(video, video_sampler, input.uv).rgb);
  return vec4<f32>(srgb_encode(BT2020_TO_BT709 * nits / 203.0), 1.0);
}

@fragment
fn empty_main() -> @location(0) vec4<f32> {
  // Hide frames invalidated by a target change rather than interpreting stale
  // SDR code values as PQ (or vice versa).
  return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}
