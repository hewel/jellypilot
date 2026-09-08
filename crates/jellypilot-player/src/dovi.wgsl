// Independent mathematical implementation. Color pipeline reference:
// libplacebo 3330a515 colorspace.c / shaders/colorspace.c, linear PQ tone
// compression and saturation rendering intent. No libplacebo code is embedded.
struct Piece {
  coefficients: vec4<f32>,
  mmr: array<vec4<f32>, 6>,
}
struct Curve {
  bounds: vec4<f32>,
  pivots: array<vec4<f32>, 2>,
  pieces: array<Piece, 8>,
}
struct DoviUniform {
  nonlinear: array<vec4<f32>, 3>,
  offset: vec4<f32>,
  linear: array<vec4<f32>, 3>,
  tone: vec4<f32>,
  output: vec4<f32>,
  curves: array<Curve, 3>,
}
@group(0) @binding(0) var luma: texture_2d<u32>;
@group(0) @binding(1) var chroma: texture_2d<u32>;
@group(0) @binding(2) var<uniform> dovi: DoviUniform;

struct VertexOutput {
  @builtin(position) position: vec4<f32>,
  @location(0) uv: vec2<f32>,
}
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOutput {
  let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
  var result: VertexOutput;
  result.position = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
  result.uv = uv;
  return result;
}

fn load_y(position: vec2<i32>) -> f32 {
  let pixel = clamp(position, vec2<i32>(0), vec2<i32>(textureDimensions(luma)) - vec2<i32>(1));
  return f32(textureLoad(luma, pixel, 0).r >> 6u) / 1023.0;
}
fn load_uv(position: vec2<i32>) -> vec2<f32> {
  let pixel = clamp(position, vec2<i32>(0), vec2<i32>(textureDimensions(chroma)) - vec2<i32>(1));
  return vec2<f32>(textureLoad(chroma, pixel, 0).rg >> vec2<u32>(6u)) / 1023.0;
}
fn sample_signal(uv: vec2<f32>) -> vec3<f32> {
  let y_pos = uv * vec2<f32>(textureDimensions(luma)) - vec2<f32>(0.5);
  let y_base = vec2<i32>(floor(y_pos));
  let y_mix = fract(y_pos);
  let y = mix(mix(load_y(y_base), load_y(y_base + vec2<i32>(1, 0)), y_mix.x),
              mix(load_y(y_base + vec2<i32>(0, 1)), load_y(y_base + vec2<i32>(1, 1)), y_mix.x), y_mix.y);
  let c_pos = uv * vec2<f32>(textureDimensions(luma)) * 0.5 - vec2<f32>(0.5) + dovi.output.yz;
  let c_base = vec2<i32>(floor(c_pos));
  let c_mix = fract(c_pos);
  let c = mix(mix(load_uv(c_base), load_uv(c_base + vec2<i32>(1, 0)), c_mix.x),
              mix(load_uv(c_base + vec2<i32>(0, 1)), load_uv(c_base + vec2<i32>(1, 1)), c_mix.x), c_mix.y);
  return clamp(vec3<f32>(y, c), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn reshape(signal: vec3<f32>, component: u32) -> f32 {
  let x = signal[component];
  let count = u32(dovi.curves[component].bounds.z);
  var index = 0u;
  for (var pivot = 0u; pivot + 1u < count; pivot += 1u) {
    if x >= dovi.curves[component].pivots[pivot / 4u][pivot % 4u] {
      index = pivot + 1u;
    }
  }
  let coefficients = dovi.curves[component].pieces[index].coefficients;
  var value = coefficients.x;
  if coefficients.w == 0.0 {
    value += x * (coefficients.y + x * coefficients.z);
  } else {
    let cross = vec4<f32>(signal.x * signal.y, signal.x * signal.z,
                         signal.y * signal.z, signal.x * signal.y * signal.z);
    var singles = signal;
    var products = cross;
    for (var power = 0u; power < u32(coefficients.w); power += 1u) {
      value += dot(dovi.curves[component].pieces[index].mmr[power * 2u].xyz, singles);
      value += dot(dovi.curves[component].pieces[index].mmr[power * 2u + 1u], products);
      singles *= signal;
      products *= cross;
    }
  }
  return clamp(value, dovi.curves[component].bounds.x, dovi.curves[component].bounds.y);
}

// ST 2084; absolute light in cd/m^2. Clamp the PQ signal before fractional
// powers so malformed excursions cannot introduce NaN into the render target.
fn pq_decode(signal: vec3<f32>) -> vec3<f32> {
  let p = pow(clamp(signal, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(32.0 / 2523.0));
  let ratio = max(p - vec3<f32>(3424.0 / 4096.0), vec3<f32>(0.0)) /
              (vec3<f32>(2413.0 / 128.0) - (2392.0 / 128.0) * p);
  return 10000.0 * pow(ratio, vec3<f32>(16384.0 / 2610.0));
}
fn pq_encode(nits: vec3<f32>) -> vec3<f32> {
  let p = pow(max(nits / 10000.0, vec3<f32>(0.0)), vec3<f32>(2610.0 / 16384.0));
  return pow((vec3<f32>(3424.0 / 4096.0) + (2413.0 / 128.0) * p) /
             (vec3<f32>(1.0) + (2392.0 / 128.0) * p), vec3<f32>(2523.0 / 32.0));
}

fn sdr_color(rgb: vec3<f32>) -> vec3<f32> {
  // BT.2020 -> XYZ(D65) -> HPE LMS with 4% inter-channel crosstalk.
  // Coefficients derived from the standard xy primaries, not a DV default.
  let lms = vec3<f32>(dot(vec3<f32>(0.41203639, 0.52391191, 0.06405498), rgb),
                      dot(vec3<f32>(0.16666022, 0.72039521, 0.11294612), rgb),
                      dot(vec3<f32>(0.02411236, 0.07547496, 0.90040794), rgb));
  let perceptual = pq_encode(lms);
  let intensity = dot(vec3<f32>(0.4, 0.4, 0.2), perceptual);
  var opponents = vec2<f32>(dot(vec3<f32>(4.455, -4.851, 0.396), perceptual),
                            dot(vec3<f32>(0.8056, 0.3572, -1.1628), perceptual));
  let mapped = mix(dovi.tone.z, dovi.tone.w,
    clamp((intensity - dovi.tone.x) / (dovi.tone.y - dovi.tone.x), 0.0, 1.0));
  // IPT gamut-volume adaptation: h(I)=I*(3-I)^2. This contracts chroma
  // during strong compression instead of retaining impossible HDR saturation.
  let old_volume = intensity * (3.0 - intensity) * (3.0 - intensity);
  let new_volume = mapped * (3.0 - mapped) * (3.0 - mapped);
  opponents *= min(intensity / max(mapped, 1e-8), new_volume / max(old_volume, 1e-8));
  let mapped_lms = pq_decode(vec3<f32>(
    mapped + dot(vec2<f32>(0.0975689, 0.205226), opponents),
    mapped + dot(vec2<f32>(-0.1138760, 0.133217), opponents),
    mapped + dot(vec2<f32>(0.0326151, -0.676887), opponents)));
  // Saturation rendering intent maps source RGB coordinates directly to the
  // target BT.709 cube. Deliberate hue/colorimetric tradeoff, matching the
  // native libplacebo fast path; this is not relative-colorimetric clipping.
  let target_nits = vec3<f32>(
    dot(vec3<f32>(3.43681483, -2.50677380, 0.06995193), mapped_lms),
    dot(vec3<f32>(-0.79105824, 1.98360167, -0.19254483), mapped_lms),
    dot(vec3<f32>(-0.02572681, -0.09914177, 1.12487414), mapped_lms));
  return clamp((target_nits - vec3<f32>(0.203)) / (203.0 - 0.203), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
  let signal = sample_signal(input.uv);
  let shaped = vec3<f32>(reshape(signal, 0u), reshape(signal, 1u), reshape(signal, 2u));
  let centered = shaped - dovi.offset.xyz;
  let nonlinear_rgb = vec3<f32>(dot(dovi.nonlinear[0].xyz, centered),
                                dot(dovi.nonlinear[1].xyz, centered),
                                dot(dovi.nonlinear[2].xyz, centered));
  let light = pq_decode(nonlinear_rgb);
  let lms = vec3<f32>(dot(dovi.linear[0].xyz, light), dot(dovi.linear[1].xyz, light), dot(dovi.linear[2].xyz, light));
  // Dolby Vision's HPE LMS reference is BT.2020. Frame RGB->LMS is applied
  // directly, not inverted, before this standard inverse HPE matrix.
  let rgb2020 = vec3<f32>(dot(vec3<f32>(3.06441879, -2.16597676, 0.10155818), lms),
                          dot(vec3<f32>(-0.65612108, 1.78554118, -0.12943749), lms),
                          dot(vec3<f32>(0.01736321, -0.04725154, 1.03004253), lms));
  var color = sdr_color(max(rgb2020, vec3<f32>(0.0)));
  if dovi.output.x == 0.0 {
    color = select(1.055 * pow(color, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055),
                   12.92 * color, color <= vec3<f32>(0.0031308));
  }
  return vec4<f32>(color, 1.0);
}
