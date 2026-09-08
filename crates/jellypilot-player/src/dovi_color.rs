//! Metadata-driven Profile 5 color conversion for the isolated player.
//!
//! Independent implementation of fixed-point DV reshaping and standard PQ/IPT
//! math. Reference: libplacebo 3330a515, colorspace.c and shaders/colorspace.c.
//! The selected rendering intent is saturation, not relative colorimetric:
//! source RGB coordinates are mapped onto BT.709 primaries after tone mapping.

use bytemuck::{Pod, Zeroable};
use dolby_vision::rpu::rpu_data_mapping::DoviMappingMethod;

use crate::{dovi::DoviFrame, playback::PlaybackError};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Piece {
  // Polynomial x^0..x^2, or MMR constant; w = MMR order (zero for polynomial).
  coefficients: [f32; 4],
  // Each order: three single-channel powers, then four cross-product powers.
  mmr: [[f32; 4]; 6],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Curve {
  // Lower bound, upper bound, piece count, reserved.
  bounds: [f32; 4],
  pivots: [[f32; 4]; 2],
  pieces: [Piece; 8],
}

/// All members have WGSL vec4 alignment; no implicit padding or heap storage.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct DoviUniform {
  nonlinear: [[f32; 4]; 3],
  offset: [f32; 4],
  linear: [[f32; 4]; 3],
  // Input PQ black/white and output PQ black/white.
  tone: [f32; 4],
  // Hardware sRGB encoding, chroma offset x/y, reserved.
  output: [f32; 4],
  curves: [Curve; 3],
}

impl DoviUniform {
  pub(crate) const BYTE_SIZE: u64 = std::mem::size_of::<Self>() as u64;

  pub(crate) fn as_bytes(&self) -> &[u8] {
    bytemuck::bytes_of(self)
  }

  pub(crate) fn new(
    frame: &DoviFrame,
    output_is_srgb: bool,
    chroma_offset: [f32; 2],
  ) -> Result<Self, PlaybackError> {
    let header = &frame.rpu.header;
    let mapping = &frame.mapping;
    if frame.rpu.dovi_profile != 5
      || header.rpu_type != 2
      || header.bl_bit_depth_minus8 != 2
      || !header.bl_video_full_range_flag
      || !header.disable_residual_flag
      || header.coefficient_data_type != 0
      || header.coefficient_log2_denom > 23
      || header.ext_mapping_idc_0_4 != 0
      || header.ext_mapping_idc_5_7 != 0
      || header.spatial_resampling_filter_flag
      || header.chroma_resampling_explicit_filter_flag
      || mapping.nlq.is_some()
      || mapping.nlq_method_idc.is_some()
      || mapping.mapping_color_space != 0
      || mapping.mapping_chroma_format_idc != 0
      || mapping.num_x_partitions_minus1 != 0
      || mapping.num_y_partitions_minus1 != 0
    {
      return Err(unsupported("unsupported Profile 5 mapping/header"));
    }
    if !chroma_offset
      .iter()
      .all(|value| value.is_finite() && value.abs() <= 0.5)
    {
      return Err(unsupported("invalid chroma sample location"));
    }
    let dm = frame
      .rpu
      .vdr_dm_data
      .as_ref()
      .ok_or_else(|| unsupported("frame has no display metadata"))?;
    if dm.compressed
      || dm.affected_dm_metadata_id != dm.current_dm_metadata_id
      || dm.current_dm_metadata_id > 15
      || dm.signal_eotf != 65535
      || dm.signal_eotf_param0 != 0
      || dm.signal_eotf_param1 != 0
      || dm.signal_eotf_param2 != 0
      || !(8..=16).contains(&dm.signal_bit_depth)
      || dm.signal_color_space != 2
      || dm.signal_chroma_format != 0
      || dm.signal_full_range_flag != 1
      || dm.source_max_pq > 4095
      || dm.source_min_pq >= dm.source_max_pq
    {
      return Err(unsupported("unsupported Profile 5 display metadata"));
    }
    let mut uniform = Self::zeroed();
    uniform.nonlinear = matrix(
      [
        dm.ycc_to_rgb_coef0,
        dm.ycc_to_rgb_coef1,
        dm.ycc_to_rgb_coef2,
        dm.ycc_to_rgb_coef3,
        dm.ycc_to_rgb_coef4,
        dm.ycc_to_rgb_coef5,
        dm.ycc_to_rgb_coef6,
        dm.ycc_to_rgb_coef7,
        dm.ycc_to_rgb_coef8,
      ],
      8192.0,
    );
    uniform.offset = [
      dm.ycc_to_rgb_offset0 as f32 / 268_435_456.0,
      dm.ycc_to_rgb_offset1 as f32 / 268_435_456.0,
      dm.ycc_to_rgb_offset2 as f32 / 268_435_456.0,
      0.0,
    ];
    uniform.linear = matrix(
      [
        dm.rgb_to_lms_coef0,
        dm.rgb_to_lms_coef1,
        dm.rgb_to_lms_coef2,
        dm.rgb_to_lms_coef3,
        dm.rgb_to_lms_coef4,
        dm.rgb_to_lms_coef5,
        dm.rgb_to_lms_coef6,
        dm.rgb_to_lms_coef7,
        dm.rgb_to_lms_coef8,
      ],
      16384.0,
    );
    // Native libplacebo SDR display, 203 nits and 1000:1 contrast. Static
    // mastering endpoints are real current-frame DM fields, matching
    // tone_map_metadata=hdr10 (not the optional level-1 scene maximum).
    let target_min = pq_encode(0.203);
    let target_max = pq_encode(203.0);
    // 7.360.1 retains its finite HDR-black sentinel in the tone curve and
    // limits the output peak for under-reported mastering displays.
    let source_max = (f32::from(dm.source_max_pq) / 4095.0).max(pq_encode(100.0));
    uniform.tone = [
      (f32::from(dm.source_min_pq) / 4095.0).max(pq_encode(1e-6)),
      source_max,
      target_min,
      target_max.min(source_max),
    ];
    uniform.output = [
      if output_is_srgb { 1.0 } else { 0.0 },
      chroma_offset[0],
      chroma_offset[1],
      0.0,
    ];
    let denominator = 1_u64 << header.coefficient_log2_denom;
    for (curve, source) in uniform.curves.iter_mut().zip(&mapping.curves) {
      if source.num_pivots_minus2 > 7
        || source.pivots.len() != source.num_pivots_minus2 as usize + 2
      {
        return Err(unsupported("invalid reshape pivot count"));
      }
      let pivots = cumulative_pivots(&source.pivots)?;
      let count = source.pivots.len() - 1;
      curve.bounds = [pivots[0], pivots[count], count as f32, 0.0];
      for (i, pivot) in pivots.iter().enumerate().take(count).skip(1) {
        curve.pivots[(i - 1) / 4][(i - 1) % 4] = *pivot;
      }
      match source.mapping_idc {
        DoviMappingMethod::Polynomial => {
          let polynomial = source
            .polynomial
            .as_ref()
            .ok_or_else(|| unsupported("missing polynomial"))?;
          if source.mmr.is_some()
            || polynomial.poly_order_minus1.len() != count
            || polynomial.linear_interp_flag.len() != count
            || polynomial.poly_coef_int.len() != count
            || polynomial.poly_coef.len() != count
          {
            return Err(unsupported("mixed or malformed polynomial mapping"));
          }
          for (i, piece) in curve.pieces[..count].iter_mut().enumerate() {
            let order = polynomial.poly_order_minus1[i];
            if order > 1
              || polynomial.linear_interp_flag[i]
              || polynomial.poly_coef_int[i].len() != order as usize + 2
              || polynomial.poly_coef[i].len() != order as usize + 2
            {
              return Err(unsupported("unsupported polynomial order/interpolation"));
            }
            for j in 0..order as usize + 2 {
              piece.coefficients[j] = fixed(
                polynomial.poly_coef_int[i][j],
                polynomial.poly_coef[i][j],
                denominator,
              )?;
            }
          }
        }
        DoviMappingMethod::MMR => {
          let mmr = source
            .mmr
            .as_ref()
            .ok_or_else(|| unsupported("missing MMR"))?;
          if source.polynomial.is_some()
            || mmr.mmr_order_minus1.len() != count
            || mmr.mmr_constant_int.len() != count
            || mmr.mmr_constant.len() != count
            || mmr.mmr_coef_int.len() != count
            || mmr.mmr_coef.len() != count
          {
            return Err(unsupported("mixed or malformed MMR mapping"));
          }
          for (i, piece) in curve.pieces[..count].iter_mut().enumerate() {
            let order = usize::from(mmr.mmr_order_minus1[i]) + 1;
            if order > 3 || mmr.mmr_coef_int[i].len() != order || mmr.mmr_coef[i].len() != order {
              return Err(unsupported("unsupported MMR order"));
            }
            piece.coefficients[0] =
              fixed(mmr.mmr_constant_int[i], mmr.mmr_constant[i], denominator)?;
            piece.coefficients[3] = order as f32;
            for power in 0..order {
              if mmr.mmr_coef_int[i][power].len() != 7 || mmr.mmr_coef[i][power].len() != 7 {
                return Err(unsupported("invalid MMR coefficient count"));
              }
              for j in 0..7 {
                let (row, column) = if j < 3 {
                  (power * 2, j)
                } else {
                  (power * 2 + 1, j - 3)
                };
                piece.mmr[row][column] = fixed(
                  mmr.mmr_coef_int[i][power][j],
                  mmr.mmr_coef[i][power][j],
                  denominator,
                )?;
              }
            }
          }
        }
        DoviMappingMethod::Invalid => return Err(unsupported("invalid reshape method")),
      }
    }
    Ok(uniform)
  }
}

fn unsupported(reason: &str) -> PlaybackError {
  PlaybackError::Frame(format!("Dolby Vision: {reason}"))
}

fn matrix(values: [i16; 9], denominator: f32) -> [[f32; 4]; 3] {
  std::array::from_fn(|row| {
    [
      f32::from(values[row * 3]) / denominator,
      f32::from(values[row * 3 + 1]) / denominator,
      f32::from(values[row * 3 + 2]) / denominator,
      0.0,
    ]
  })
}

fn cumulative_pivots(deltas: &[u16]) -> Result<[f32; 9], PlaybackError> {
  if !(2..=9).contains(&deltas.len()) {
    return Err(unsupported("invalid pivot count"));
  }
  let mut result = [0.0; 9];
  let mut sum = 0_u32;
  for (i, delta) in deltas.iter().enumerate() {
    sum += u32::from(*delta);
    if sum > 1023 || (i > 0 && *delta == 0) {
      return Err(unsupported(
        "reshape pivots are not strictly increasing 10-bit values",
      ));
    }
    result[i] = sum as f32 / 1023.0;
  }
  Ok(result)
}

fn fixed(integer: i64, fraction: u64, denominator: u64) -> Result<f32, PlaybackError> {
  // FFmpeg's fixed coefficients use a signed 64-bit numerator. Reject inputs
  // outside that same arithmetic domain before converting to GPU floats.
  let numerator = i128::from(integer) * i128::from(denominator) + i128::from(fraction);
  if fraction >= denominator || i64::try_from(numerator).is_err() {
    return Err(unsupported("invalid fixed-point reshape coefficient"));
  }
  Ok((integer as f64 + fraction as f64 / denominator as f64) as f32)
}

fn pq_encode(nits: f32) -> f32 {
  let power = (nits / 10_000.0).powf(2610.0 / 16384.0);
  ((3424.0 / 4096.0 + 2413.0 / 128.0 * power) / (1.0 + 2392.0 / 128.0 * power)).powf(2523.0 / 32.0)
}

#[cfg(test)]
mod tests {
  use super::{cumulative_pivots, fixed};

  #[test]
  fn reshape_pivots_accumulate_deltas_and_reject_overflow() {
    let pivots = cumulative_pivots(&[2, 256, 23]).expect("valid deltas");
    assert_eq!(
      &pivots[..3],
      &[2.0 / 1023.0, 258.0 / 1023.0, 281.0 / 1023.0]
    );
    assert!(cumulative_pivots(&[1023, 1]).is_err());
    assert!(cumulative_pivots(&[0, 0]).is_err());
  }

  #[test]
  fn signed_fixed_coefficients_add_fraction_instead_of_sign_extending_it() {
    assert_eq!(fixed(-2, 3, 4).expect("valid signed coefficient"), -1.25);
    assert!(fixed(0, 4, 4).is_err());
    assert!(fixed(i64::MAX, 0, 4).is_err());
  }
}
