//! Background image → AVIF encoding for the Library Image cache.
//!
//! Encoding runs entirely off the image-serving path. A completed original is
//! sniffed from its bytes (never a URL extension or the untrusted
//! Content-Type), decoded, normalized, and encoded to an 8-bit AVIF under the
//! fixed policy (color quality 80, lossless alpha, speed 8, one thread). EXIF
//! orientation is applied to pixels before encoding, so dimensions may swap.
//! Output is structurally parsed and dimension- and alpha-checked before
//! activation. The serving path never decodes or encodes.

use image::ImageDecoder as _;
use ravif::{BitDepth, Encoder, Img, RGB8, RGBA8};

/// Encoded-source size limit (bytes).
pub const ENCODE_MAX_SOURCE_BYTES: u64 = 32 * 1024 * 1024;
/// Decoded pixel-count limit.
pub const ENCODE_MAX_PIXELS: u64 = 24_000_000;
/// Maximum single dimension (px).
pub const ENCODE_MAX_DIMENSION: u32 = 12_000;
/// Color quality (1-100, higher is better).
pub const ENCODE_QUALITY: f32 = 80.0;
/// Alpha quality: 100 is lossless so transparency survives bit-exact.
pub const ENCODE_ALPHA_QUALITY: f32 = 100.0;
/// Encoder speed (1-10, higher is faster).
pub const ENCODE_SPEED: u8 = 8;
/// Minimum fractional saving for AVIF to become active.
pub const MIN_SAVING_FRACTION: f64 = 0.15;

/// Why a source cannot or should not be converted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeReject {
  /// Policy exclusion: a valid but ineligible input (wrong or animated format,
  /// unsafe color, or over an admission limit). Terminal, not retried.
  NotEligible,
  /// Invalid source data: bytes that do not form a decodable image. Terminal,
  /// not retried.
  Corrupt,
  /// Transient encoder/resource failure. Eligible for retry.
  Transient,
}

/// A successfully encoded AVIF.
pub struct EncodedAvif {
  pub bytes: Vec<u8>,
  pub width: u32,
  pub height: u32,
  /// True when the source carried real transparency, encoded as a lossless
  /// alpha plane. Opaque sources encode without one.
  pub has_alpha: bool,
}

/// Decode a static JPEG, PNG, or WebP source and encode it to AVIF under the
/// fixed policy. Eligibility is decided from the image bytes; orientation is
/// applied to pixels (swapping dimensions) before encoding; alpha is preserved
/// losslessly while color uses quality 80. No non-visual metadata is copied.
pub fn encode_image_to_avif(source: &[u8]) -> Result<EncodedAvif, EncodeReject> {
  if source.len() as u64 > ENCODE_MAX_SOURCE_BYTES {
    return Err(EncodeReject::NotEligible);
  }

  let format = match image::guess_format(source) {
    Ok(format) => format,
    Err(_) => {
      // SVG is XML and is not in the raster magic-byte table.
      return Err(if is_svg(source) {
        EncodeReject::NotEligible
      } else {
        EncodeReject::Corrupt
      });
    }
  };

  match format {
    image::ImageFormat::Jpeg | image::ImageFormat::Png => {}
    image::ImageFormat::WebP => {
      if is_animated_webp(source) {
        return Err(EncodeReject::NotEligible);
      }
    }
    // GIF, origin AVIF, and every other recognized image format stays a usable
    // origin rather than being re-encoded.
    _ => return Err(EncodeReject::NotEligible),
  }

  // Decode and apply EXIF orientation to pixels (may swap width/height).
  let mut decoder = image::ImageReader::new(std::io::Cursor::new(source))
    .with_guessed_format()
    .map_err(|_| EncodeReject::Corrupt)?
    .into_decoder()
    .map_err(|_| EncodeReject::Corrupt)?;
  let orientation = decoder
    .orientation()
    .unwrap_or(image::metadata::Orientation::NoTransforms);
  let mut image = image::DynamicImage::from_decoder(decoder).map_err(|_| EncodeReject::Corrupt)?;
  image.apply_orientation(orientation);

  let width = image.width();
  let height = image.height();
  if width > ENCODE_MAX_DIMENSION
    || height > ENCODE_MAX_DIMENSION
    || (width as u64) * (height as u64) > ENCODE_MAX_PIXELS
  {
    return Err(EncodeReject::NotEligible);
  }

  let has_alpha = matches!(
    image.color(),
    image::ColorType::La8
      | image::ColorType::La16
      | image::ColorType::Rgba8
      | image::ColorType::Rgba16
  );

  // Anything we cannot normalize to 8-bit sRGB (float HDR or exotic color) is
  // left as a usable origin rather than encoded with the wrong color.
  let representable = has_alpha
    || matches!(
      image.color(),
      image::ColorType::L8
        | image::ColorType::L16
        | image::ColorType::Rgb8
        | image::ColorType::Rgb16
    );
  if !representable {
    return Err(EncodeReject::NotEligible);
  }

  let encoder = Encoder::new()
    .with_quality(ENCODE_QUALITY)
    .with_alpha_quality(ENCODE_ALPHA_QUALITY)
    .with_speed(ENCODE_SPEED)
    .with_bit_depth(BitDepth::Eight)
    .with_num_threads(Some(1));

  if has_alpha {
    let rgba = image.to_rgba8();
    let raw = rgba.into_raw();
    let pixels: Vec<RGBA8> = raw
      .chunks_exact(4)
      .map(|c| RGBA8 {
        r: c[0],
        g: c[1],
        b: c[2],
        a: c[3],
      })
      .collect();
    let result = encoder
      .encode_rgba(Img::new(pixels.as_slice(), width as usize, height as usize))
      .map_err(|_| EncodeReject::Transient)?;
    Ok(EncodedAvif {
      bytes: result.avif_file,
      width,
      height,
      has_alpha: result.alpha_byte_size > 0,
    })
  } else {
    let rgb = image.to_rgb8();
    let raw = rgb.into_raw();
    let pixels: Vec<RGB8> = raw
      .chunks_exact(3)
      .map(|c| RGB8 {
        r: c[0],
        g: c[1],
        b: c[2],
      })
      .collect();
    let result = encoder
      .encode_rgb(Img::new(pixels.as_slice(), width as usize, height as usize))
      .map_err(|_| EncodeReject::Transient)?;
    Ok(EncodedAvif {
      bytes: result.avif_file,
      width,
      height,
      has_alpha: false,
    })
  }
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

/// Structural AVIF validation: true when the container declares an auxiliary
/// alpha channel (`auxC` with the alpha URN), so transparency expectations can
/// be checked before activation without a full decoder.
pub fn avif_has_alpha(avif: &[u8]) -> bool {
  const ALPHA_AUX_URN: &[u8] = b"urn:mpeg:mpegB:cicp:systems:auxiliary:alpha";
  avif
    .windows(ALPHA_AUX_URN.len())
    .any(|window| window == ALPHA_AUX_URN)
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

/// True when the bytes look like an SVG document (XML), which is not a raster
/// image the still-image policy can encode.
fn is_svg(bytes: &[u8]) -> bool {
  let mut head = bytes;
  // Skip a UTF-8 BOM and leading ASCII whitespace.
  if head.starts_with(&[0xEF, 0xBB, 0xBF]) {
    head = &head[3..];
  }
  while let Some((&c, rest)) = head.split_first() {
    if c.is_ascii_whitespace() {
      head = rest;
    } else {
      break;
    }
  }
  let head = &head[..head.len().min(4096)];
  let lower: Vec<u8> = head.iter().map(u8::to_ascii_lowercase).collect();
  lower.starts_with(b"<svg") || (lower.starts_with(b"<?xml") && contains(&lower, b"<svg"))
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
  haystack.windows(needle.len()).any(|w| w == needle)
}

/// True when a WebP RIFF container carries animation (a VP8X animation flag or
/// ANIM/ANMF chunks). Animated WebP must not be flattened to a single frame.
fn is_animated_webp(bytes: &[u8]) -> bool {
  if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
    return false;
  }
  let mut offset = 12;
  while offset + 8 <= bytes.len() {
    let fourcc = &bytes[offset..offset + 4];
    if fourcc == b"ANIM" || fourcc == b"ANMF" {
      return true;
    }
    if fourcc == b"VP8X" {
      // VP8X feature flags byte: bit 1 (0x02) signals animation.
      if bytes.get(offset + 8).is_some_and(|flags| flags & 0x02 != 0) {
        return true;
      }
    }
    let size = u32::from_le_bytes([
      bytes[offset + 4],
      bytes[offset + 5],
      bytes[offset + 6],
      bytes[offset + 7],
    ]) as usize;
    // Chunk payloads are padded to even sizes.
    let advance = 8 + size + (size & 1);
    offset = offset.saturating_add(advance);
  }
  false
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

  fn make_jpeg(width: u32, height: u32) -> Vec<u8> {
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
    jpeg
  }

  fn make_png(width: u32, height: u32, alpha: bool) -> Vec<u8> {
    let (mut px, color) = if alpha {
      (
        vec![0u8; (width * height * 4) as usize],
        image::ColorType::Rgba8,
      )
    } else {
      (
        vec![0u8; (width * height * 3) as usize],
        image::ColorType::Rgb8,
      )
    };
    if alpha {
      for (i, chunk) in px.chunks_exact_mut(4).enumerate() {
        chunk[0] = (i as u8).wrapping_mul(3);
        chunk[1] = 200;
        chunk[2] = 20;
        chunk[3] = if i % 2 == 0 { 0 } else { 255 };
      }
    } else {
      for (i, chunk) in px.chunks_exact_mut(3).enumerate() {
        chunk[0] = (i as u8).wrapping_mul(3);
        chunk[1] = 200;
        chunk[2] = 20;
      }
    }
    let mut png = Vec::new();
    image::write_buffer_with_format(
      &mut std::io::Cursor::new(&mut png),
      &px,
      width,
      height,
      color,
      image::ImageFormat::Png,
    )
    .expect("encode png");
    png
  }

  fn make_webp(width: u32, height: u32) -> Vec<u8> {
    let mut rgb = vec![0u8; (width * height * 3) as usize];
    for (i, chunk) in rgb.chunks_exact_mut(3).enumerate() {
      chunk[0] = (i as u8).wrapping_mul(5);
      chunk[1] = 90;
      chunk[2] = 160;
    }
    let mut webp = Vec::new();
    image::write_buffer_with_format(
      &mut std::io::Cursor::new(&mut webp),
      &rgb,
      width,
      height,
      image::ColorType::Rgb8,
      image::ImageFormat::WebP,
    )
    .expect("encode webp");
    webp
  }

  fn make_gif() -> Vec<u8> {
    // Minimal GIF89a header; eligibility is decided on magic bytes alone.
    b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\x00\x00\x00;".to_vec()
  }

  fn make_svg() -> Vec<u8> {
    br#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"></svg>"#
      .to_vec()
  }

  fn make_animated_webp() -> Vec<u8> {
    // RIFF/WEBP with a VP8X chunk whose animation flag is set. Eligibility is
    // decided on the container structure, so a full valid frame is unnecessary.
    let mut v = Vec::new();
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&[0u8; 4]); // size (unused)
    v.extend_from_slice(b"WEBP");
    v.extend_from_slice(b"VP8X");
    v.extend_from_slice(&10u32.to_le_bytes()); // chunk size
    v.push(0x02); // feature flags: animation bit
    v.extend_from_slice(&[0u8; 9]);
    v
  }

  // Insert an EXIF orientation tag into a JPEG produced by `make_jpeg`.
  fn with_exif_orientation(jpeg: &[u8], orientation: u16) -> Vec<u8> {
    // TIFF block (little-endian) with a single orientation entry.
    let mut tiff = Vec::new();
    tiff.extend_from_slice(b"II");
    tiff.extend_from_slice(&42u16.to_le_bytes());
    tiff.extend_from_slice(&8u32.to_le_bytes()); // IFD offset
    tiff.extend_from_slice(&1u16.to_le_bytes()); // one entry
    tiff.extend_from_slice(&0x0112u16.to_le_bytes()); // orientation tag
    tiff.extend_from_slice(&3u16.to_le_bytes()); // SHORT
    tiff.extend_from_slice(&1u32.to_le_bytes()); // count
    tiff.extend_from_slice(&orientation.to_le_bytes());
    tiff.extend_from_slice(&[0u8; 2]); // padding
    tiff.extend_from_slice(&[0u8; 4]); // next IFD offset

    let mut app1 = Vec::new();
    app1.extend_from_slice(&[0xFF, 0xE1]);
    let payload_len = (6 + tiff.len() + 2) as u16;
    app1.extend_from_slice(&payload_len.to_be_bytes());
    app1.extend_from_slice(b"Exif\0\0");
    app1.extend_from_slice(&tiff);

    // Insert after the SOI marker (FF D8).
    let mut out = Vec::with_capacity(jpeg.len() + app1.len());
    out.extend_from_slice(&jpeg[..2]);
    out.extend_from_slice(&app1);
    out.extend_from_slice(&jpeg[2..]);
    out
  }

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
  fn corrupt_source_is_failed_not_ineligible() {
    let result = encode_image_to_avif(b"this is not a jpeg");
    assert_eq!(result.err(), Some(EncodeReject::Corrupt));
  }

  #[test]
  fn oversized_source_is_not_eligible() {
    let big = vec![0u8; (ENCODE_MAX_SOURCE_BYTES + 1) as usize];
    assert_eq!(
      encode_image_to_avif(&big).err(),
      Some(EncodeReject::NotEligible)
    );
  }

  #[test]
  fn opaque_jpeg_round_trips_to_valid_avif() {
    let jpeg = make_jpeg(32, 32);
    let encoded = encode_image_to_avif(&jpeg).expect("opaque jpeg must encode");
    assert_eq!((encoded.width, encoded.height), (32, 32));
    assert!(!encoded.has_alpha, "opaque jpeg has no alpha plane");
    assert!(is_avif_container(&encoded.bytes));
    let (w, h) = parse_avif_dimensions(&encoded.bytes).expect("dims parse");
    assert_eq!((w, h), (32, 32));
    assert!(!avif_has_alpha(&encoded.bytes));
  }

  #[test]
  fn opaque_png_round_trips_without_alpha() {
    let png = make_png(24, 16, false);
    let encoded = encode_image_to_avif(&png).expect("opaque png must encode");
    assert_eq!((encoded.width, encoded.height), (24, 16));
    assert!(!encoded.has_alpha);
    assert!(!avif_has_alpha(&encoded.bytes));
  }

  #[test]
  fn transparent_png_preserves_lossless_alpha() {
    let png = make_png(16, 16, true);
    let encoded = encode_image_to_avif(&png).expect("transparent png must encode");
    assert!(encoded.has_alpha, "alpha must be preserved");
    assert!(
      avif_has_alpha(&encoded.bytes),
      "container must declare an alpha auxiliary channel"
    );
    let (w, h) = parse_avif_dimensions(&encoded.bytes).expect("dims parse");
    assert_eq!((w, h), (16, 16));
  }

  #[test]
  fn static_webp_is_eligible() {
    let webp = make_webp(16, 16);
    let encoded = encode_image_to_avif(&webp).expect("static webp must encode");
    assert_eq!((encoded.width, encoded.height), (16, 16));
  }

  #[test]
  fn animated_webp_is_not_eligible() {
    let webp = make_animated_webp();
    assert_eq!(
      encode_image_to_avif(&webp).err(),
      Some(EncodeReject::NotEligible)
    );
  }

  #[test]
  fn gif_is_not_eligible() {
    assert_eq!(
      encode_image_to_avif(&make_gif()).err(),
      Some(EncodeReject::NotEligible)
    );
  }

  #[test]
  fn origin_avif_is_not_eligible() {
    // A real AVIF produced by the encoder is itself an origin AVIF.
    let avif = encode_image_to_avif(&make_jpeg(16, 16))
      .expect("seed avif")
      .bytes;
    assert!(is_avif_container(&avif));
    assert_eq!(
      encode_image_to_avif(&avif).err(),
      Some(EncodeReject::NotEligible)
    );
  }

  #[test]
  fn svg_is_not_eligible() {
    assert_eq!(
      encode_image_to_avif(&make_svg()).err(),
      Some(EncodeReject::NotEligible)
    );
  }

  #[test]
  fn exif_orientation_swaps_dimensions_before_encode() {
    // 4x2 image rotated 90 degrees must become 2x4 in the output.
    let jpeg = with_exif_orientation(&make_jpeg(4, 2), 6);
    let encoded = encode_image_to_avif(&jpeg).expect("rotated jpeg must encode");
    assert_eq!(
      (encoded.width, encoded.height),
      (2, 4),
      "orientation must be applied to pixels, swapping dimensions"
    );
    let (w, h) = parse_avif_dimensions(&encoded.bytes).expect("dims parse");
    assert_eq!(
      (w, h),
      (2, 4),
      "container dims must match normalized pixels"
    );
  }

  #[test]
  fn sixteen_bit_png_is_downconverted_and_eligible() {
    let width = 8u32;
    let height = 8u32;
    let mut px = vec![0u8; (width * height * 3 * 2) as usize];
    for chunk in px.chunks_exact_mut(6) {
      chunk[0] = 0xFF;
      chunk[2] = 0x80;
      chunk[4] = 0x10;
    }
    let mut png = Vec::new();
    image::write_buffer_with_format(
      &mut std::io::Cursor::new(&mut png),
      &px,
      width,
      height,
      image::ColorType::Rgb16,
      image::ImageFormat::Png,
    )
    .expect("encode 16-bit png");
    let encoded = encode_image_to_avif(&png).expect("16-bit png must encode");
    assert_eq!((encoded.width, encoded.height), (8, 8));
  }

  #[test]
  fn non_visual_metadata_is_not_copied() {
    let jpeg = with_exif_orientation(&make_jpeg(16, 16), 6);
    let encoded = encode_image_to_avif(&jpeg).expect("encode");
    assert!(
      !encoded.bytes.windows(4).any(|w| w == b"Exif"),
      "EXIF and other non-visual metadata must not be copied into the output"
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
}
