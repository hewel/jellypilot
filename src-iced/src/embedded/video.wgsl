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
