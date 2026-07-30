//! Background JPEG → AVIF encoding for the Library Image cache.
//!
//! Encoding runs entirely off the image-serving path. A completed original is
//! decoded, admission-checked, and encoded to an 8-bit AVIF with the fixed
//! policy (quality 80, speed 8, one thread). Output is structurally parsed and
//! dimension-checked before activation. The serving path never decodes or
//! encodes.

use ravif::{BitDepth, Encoder, Img, RGB8};

/// Encoded-source size limit (bytes).
pub const ENCODE_MAX_SOURCE_BYTES: u64 = 32 * 1024 * 1024;
/// Decoded pixel-count limit.
pub const ENCODE_MAX_PIXELS: u64 = 24_000_000;
/// Maximum single dimension (px).
pub const ENCODE_MAX_DIMENSION: u32 = 12_000;
/// Color quality (1-100, higher is better).
pub const ENCODE_QUALITY: f32 = 80.0;
/// Encoder speed (1-10, higher is faster).
pub const ENCODE_SPEED: u8 = 8;
/// Minimum fractional saving for AVIF to become active.
pub const MIN_SAVING_FRACTION: f64 = 0.15;

/// Why a source cannot or should not be converted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeReject {
  /// Policy exclusion: not an opaque static JPEG, or over an admission limit.
  /// Terminal, not retried.
  NotEligible,
  /// Corrupt or undecodable source. Terminal, not retried.
  Corrupt,
  /// Transient encoder/resource failure. Eligible for retry.
  Transient,
}

/// A successfully encoded AVIF.
pub struct EncodedAvif {
  pub bytes: Vec<u8>,
  pub width: u32,
  pub height: u32,
}

/// Decode an opaque JPEG and encode it to AVIF under the fixed policy.
///
/// Returns the encoded bytes and normalized dimensions, or a rejection reason.
pub fn encode_jpeg_to_avif(jpeg: &[u8]) -> Result<EncodedAvif, EncodeReject> {
  if jpeg.len() as u64 > ENCODE_MAX_SOURCE_BYTES {
    return Err(EncodeReject::NotEligible);
  }

  let decoded = image::load_from_memory(jpeg).map_err(|_| EncodeReject::Corrupt)?;

  // Only opaque JPEG is eligible in this stage.
  if !matches!(
    decoded.color(),
    image::ColorType::Rgb8 | image::ColorType::L8
  ) {
    return Err(EncodeReject::NotEligible);
  }

  let width = decoded.width();
  let height = decoded.height();
  if width > ENCODE_MAX_DIMENSION
    || height > ENCODE_MAX_DIMENSION
    || (width as u64) * (height as u64) > ENCODE_MAX_PIXELS
  {
    return Err(EncodeReject::NotEligible);
  }

  let rgb_image = decoded.to_rgb8();
  let raw = rgb_image.into_raw();
  let pixels: Vec<RGB8> = raw
    .chunks_exact(3)
    .map(|c| RGB8 {
      r: c[0],
      g: c[1],
      b: c[2],
    })
    .collect();

  let result = Encoder::new()
    .with_quality(ENCODE_QUALITY)
    .with_alpha_quality(ENCODE_QUALITY)
    .with_speed(ENCODE_SPEED)
    .with_bit_depth(BitDepth::Eight)
    .with_num_threads(Some(1))
    .encode_rgb(Img::new(pixels.as_slice(), width as usize, height as usize))
    .map_err(|_| EncodeReject::Transient)?;

  Ok(EncodedAvif {
    bytes: result.avif_file,
    width,
    height,
  })
}

/// Whether an AVIF of `avif_size` bytes is worth activating over an original of
/// `original_size` bytes (at least a 15% reduction).
pub fn has_sufficient_saving(original_size: u64, avif_size: u64) -> bool {
  if original_size == 0 {
    return false;
  }
  let saving = 1.0 - (avif_size as f64 / original_size as f64);
  saving >= MIN_SAVING_FRACTION
}

/// Structural AVIF validation: confirm the ISOBMFF `ftyp` brand and extract the
/// coded dimensions from the `ispe` item property without a full decoder.
pub fn parse_avif_dimensions(avif: &[u8]) -> Option<(u32, u32)> {
  if !is_avif_container(avif) {
    return None;
  }
  find_ispe(avif)
}

/// True when the byte stream is an AVIF/HEIF container (`ftyp` brand `avif`).
pub fn is_avif_container(bytes: &[u8]) -> bool {
  // Minimal ISOBMFF: first box must be `ftyp`; major/compatible brand `avif`.
  let Some(ftyp) = read_box_header(bytes, 0) else {
    return false;
  };
  if &bytes[ftyp.type_offset..ftyp.type_offset + 4] != b"ftyp" {
    return false;
  }
  let body = &bytes[ftyp.body_offset..ftyp.end.min(bytes.len())];
  // major brand (4) + minor version (4), then compatible brands in 4-byte runs.
  if body.len() < 8 {
    return false;
  }
  if &body[0..4] == b"avif" {
    return true;
  }
  body[8..]
    .chunks_exact(4)
    .any(|brand| brand == b"avif" || brand == b"avis")
}

struct BoxHeader {
  type_offset: usize,
  body_offset: usize,
  end: usize,
}

fn read_box_header(bytes: &[u8], offset: usize) -> Option<BoxHeader> {
  if offset + 8 > bytes.len() {
    return None;
  }
  let size = u32::from_be_bytes([
    bytes[offset],
    bytes[offset + 1],
    bytes[offset + 2],
    bytes[offset + 3],
  ]) as usize;
  let type_offset = offset + 4;
  let (body_offset, end) = if size == 1 {
    // 64-bit largesize.
    if offset + 16 > bytes.len() {
      return None;
    }
    let large = u64::from_be_bytes([
      bytes[offset + 8],
      bytes[offset + 9],
      bytes[offset + 10],
      bytes[offset + 11],
      bytes[offset + 12],
      bytes[offset + 13],
      bytes[offset + 14],
      bytes[offset + 15],
    ]) as usize;
    (offset + 16, offset + large)
  } else {
    (offset + 8, offset + size)
  };
  Some(BoxHeader {
    type_offset,
    body_offset,
    end,
  })
}

/// Recursively scan container boxes for an `ispe` property and read its
/// `width`/`height` (two big-endian u32 after an 8-byte fullbox header).
fn find_ispe(bytes: &[u8]) -> Option<(u32, u32)> {
  let mut offset = 0;
  while let Some(header) = read_box_header(bytes, offset) {
    if header.type_offset + 4 > bytes.len() {
      return None;
    }
    let kind = &bytes[header.type_offset..header.type_offset + 4];
    match kind {
      b"ispe" => {
        // fullbox: version(1) + flags(3), then width(4) + height(4).
        let body = &bytes[header.body_offset..];
        if body.len() >= 12 {
          let width = u32::from_be_bytes([body[4], body[5], body[6], body[7]]);
          let height = u32::from_be_bytes([body[8], body[9], body[10], body[11]]);
          return Some((width, height));
        }
        return None;
      }
      // Container boxes that may hold `ispe` (meta is a fullbox: skip 4 bytes).
      b"meta" => {
        let inner_start = header.body_offset + 4;
        if let Some(found) = find_ispe(&bytes[inner_start..header.end.min(bytes.len())]) {
          return Some(found);
        }
      }
      b"moov" | b"iprp" | b"ipco" | b"trak" | b"mdia" | b"minf" | b"stbl" => {
        if let Some(found) = find_ispe(&bytes[header.body_offset..header.end.min(bytes.len())]) {
          return Some(found);
        }
      }
      _ => {}
    }
    if header.end <= offset {
      break;
    }
    offset = header.end;
  }
  None
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn sufficient_saving_threshold() {
    assert!(has_sufficient_saving(1000, 850), "exactly 15% is enough");
    assert!(has_sufficient_saving(1000, 500));
    assert!(!has_sufficient_saving(1000, 851), "14.9% is not enough");
    assert!(
      !has_sufficient_saving(0, 0),
      "zero original never activates"
    );
  }

  #[test]
  fn corrupt_source_is_corrupt_not_ineligible() {
    let result = encode_jpeg_to_avif(b"this is not a jpeg");
    assert_eq!(result.err(), Some(EncodeReject::Corrupt));
  }

  #[test]
  fn oversized_source_is_not_eligible() {
    let big = vec![0u8; (ENCODE_MAX_SOURCE_BYTES + 1) as usize];
    assert_eq!(
      encode_jpeg_to_avif(&big).err(),
      Some(EncodeReject::NotEligible)
    );
  }

  #[test]
  fn detects_avif_container_brand() {
    // Minimal ftyp box: size(4) + "ftyp" + major "avif" + minor(4).
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&16u32.to_be_bytes());
    bytes.extend_from_slice(b"ftyp");
    bytes.extend_from_slice(b"avif");
    bytes.extend_from_slice(&0u32.to_be_bytes());
    assert!(is_avif_container(&bytes));

    let mut not_avif = bytes.clone();
    not_avif[8..12].copy_from_slice(b"mp42");
    assert!(!is_avif_container(&not_avif));
    assert!(!is_avif_container(b"garbage"));
  }

  #[test]
  fn opaque_jpeg_round_trips_to_valid_avif() {
    // Build a small opaque RGB JPEG in memory.
    let width = 32u32;
    let height = 32u32;
    let mut rgb = vec![0u8; (width * height * 3) as usize];
    for (i, chunk) in rgb.chunks_exact_mut(3).enumerate() {
      let x = (i as u32 % width) as u8;
      let y = (i as u32 / width) as u8;
      chunk[0] = x.wrapping_mul(8);
      chunk[1] = y.wrapping_mul(8);
      chunk[2] = 128;
    }
    let mut jpeg = Vec::new();
    image::write_buffer_with_format(
      &mut std::io::Cursor::new(&mut jpeg),
      &rgb,
      width,
      height,
      image::ColorType::Rgb8,
      image::ImageFormat::Jpeg,
    )
    .expect("encode jpeg");

    let encoded = encode_jpeg_to_avif(&jpeg).expect("opaque jpeg must encode");
    assert_eq!(encoded.width, width);
    assert_eq!(encoded.height, height);
    assert!(
      is_avif_container(&encoded.bytes),
      "output must be an AVIF container"
    );
    let (w, h) = parse_avif_dimensions(&encoded.bytes).expect("dimensions parse");
    assert_eq!((w, h), (width, height), "parsed dims must match");
    assert!(
      has_sufficient_saving(jpeg.len() as u64, encoded.bytes.len() as u64)
        || encoded.bytes.len() < jpeg.len(),
      "small synthetic input may or may not clear the 15% bar, but must not grow absurdly"
    );
  }

  #[test]
  fn rgba_source_is_not_eligible() {
    // RGBA PNG is not an opaque JPEG -> not eligible at this stage.
    let width = 8u32;
    let height = 8u32;
    let rgba = vec![255u8; (width * height * 4) as usize];
    let mut png = Vec::new();
    image::write_buffer_with_format(
      &mut std::io::Cursor::new(&mut png),
      &rgba,
      width,
      height,
      image::ColorType::Rgba8,
      image::ImageFormat::Png,
    )
    .expect("encode png");
    assert_eq!(
      encode_jpeg_to_avif(&png).err(),
      Some(EncodeReject::NotEligible)
    );
  }
}
