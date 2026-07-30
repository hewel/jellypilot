//! Release-mode AVIF policy benchmark (#193).
//!
//! Generates a synthetic-but-representative Library Image corpus, encodes each
//! item through the fixed policy, and reports compression, encode time, peak
//! memory, threshold pass/fail, and (with `--features bench`) PSNR fidelity.
//!
//! Run:
//!   cargo run --release --example avif_bench                 # core metrics
//!   cargo run --release --example avif_bench --features bench # adds PSNR
//!
//! The output is a Markdown table suitable for `docs/avif-policy-benchmark.md`.

use std::time::Instant;

use jellypilot_lib::avif_encode;

/// One synthetic corpus item: a name plus the source bytes to convert.
struct CorpusItem {
  name: &'static str,
  source: Vec<u8>,
}

fn lcg(seed: &mut u64) -> u8 {
  *seed = seed
    .wrapping_mul(6364136223846793005)
    .wrapping_add(1442695040888963407);
  (*seed >> 33) as u8
}

/// Smooth photographic-style gradient with per-pixel noise.
fn photo_rgb(w: u32, h: u32, seed: u64) -> Vec<u8> {
  let mut s = seed;
  let mut px = vec![0u8; (w * h * 3) as usize];
  for y in 0..h {
    for x in 0..w {
      let i = ((y * w + x) * 3) as usize;
      let gx = (x * 255 / w.max(1)) as u8;
      let gy = (y * 255 / h.max(1)) as u8;
      px[i] = gx.wrapping_add(lcg(&mut s) / 6);
      px[i + 1] = gy.wrapping_add(lcg(&mut s) / 6);
      px[i + 2] = ((gx as u16 + gy as u16) / 2) as u8;
    }
  }
  px
}

/// Landscape: a bright sky band over a darker ground band.
fn landscape_rgb(w: u32, h: u32) -> Vec<u8> {
  let mut px = vec![0u8; (w * h * 3) as usize];
  for y in 0..h {
    for x in 0..w {
      let i = ((y * w + x) * 3) as usize;
      if y < h / 2 {
        px[i] = 120 + (x % 40) as u8;
        px[i + 1] = 180;
        px[i + 2] = 235;
      } else {
        px[i] = 40 + (x % 30) as u8;
        px[i + 1] = 90 + (y % 40) as u8;
        px[i + 2] = 45;
      }
    }
  }
  px
}

/// Sharp edges / text-like high-contrast blocks.
fn sharp_rgb(w: u32, h: u32) -> Vec<u8> {
  let mut px = vec![0u8; (w * h * 3) as usize];
  for y in 0..h {
    for x in 0..w {
      let i = ((y * w + x) * 3) as usize;
      let on = ((x / 8) + (y / 8)) % 2 == 0;
      let v = if on { 250u8 } else { 10u8 };
      px[i] = v;
      px[i + 1] = v;
      px[i + 2] = if on { 40 } else { 230 };
    }
  }
  px
}

/// Worst case for compression: pure random noise.
fn noise_rgb(w: u32, h: u32, seed: u64) -> Vec<u8> {
  let mut s = seed;
  let mut px = vec![0u8; (w * h * 3) as usize];
  for b in px.iter_mut() {
    *b = lcg(&mut s);
  }
  px
}

fn encode(rgb: &[u8], w: u32, h: u32, format: image::ImageFormat) -> Vec<u8> {
  let mut out = Vec::new();
  image::write_buffer_with_format(
    &mut std::io::Cursor::new(&mut out),
    rgb,
    w,
    h,
    image::ColorType::Rgb8,
    format,
  )
  .expect("encode source");
  out
}

fn encode_rgba_png(w: u32, h: u32) -> Vec<u8> {
  let mut px = vec![0u8; (w * h * 4) as usize];
  for y in 0..h {
    for x in 0..w {
      let i = ((y * w + x) * 4) as usize;
      px[i] = (x * 255 / w.max(1)) as u8;
      px[i + 1] = 90;
      px[i + 2] = 200;
      // A smooth alpha ramp with a transparent corner.
      px[i + 3] = if x < w / 4 {
        0
      } else {
        (y * 255 / h.max(1)) as u8
      };
    }
  }
  let mut out = Vec::new();
  image::write_buffer_with_format(
    &mut std::io::Cursor::new(&mut out),
    &px,
    w,
    h,
    image::ColorType::Rgba8,
    image::ImageFormat::Png,
  )
  .expect("encode rgba png");
  out
}

fn corpus() -> Vec<CorpusItem> {
  vec![
    CorpusItem {
      name: "poster-jpeg-480x720",
      source: encode(&photo_rgb(480, 720, 1), 480, 720, image::ImageFormat::Jpeg),
    },
    CorpusItem {
      name: "thumb-jpeg-320x180",
      source: encode(&landscape_rgb(320, 180), 320, 180, image::ImageFormat::Jpeg),
    },
    CorpusItem {
      name: "backdrop-jpeg-1920x1080",
      source: encode(
        &photo_rgb(1920, 1080, 7),
        1920,
        1080,
        image::ImageFormat::Jpeg,
      ),
    },
    CorpusItem {
      name: "opaque-png-400x300",
      source: encode(&photo_rgb(400, 300, 3), 400, 300, image::ImageFormat::Png),
    },
    CorpusItem {
      name: "transparent-png-400x300",
      source: encode_rgba_png(400, 300),
    },
    CorpusItem {
      name: "static-webp-400x300",
      source: encode(&landscape_rgb(400, 300), 400, 300, image::ImageFormat::WebP),
    },
    CorpusItem {
      name: "sharp-text-png-400x200",
      source: encode(&sharp_rgb(400, 200), 400, 200, image::ImageFormat::Png),
    },
    CorpusItem {
      name: "noise-jpeg-300x300",
      source: encode(&noise_rgb(300, 300, 11), 300, 300, image::ImageFormat::Jpeg),
    },
  ]
}

#[cfg(target_os = "linux")]
fn peak_hwm_kb() -> Option<u64> {
  let status = std::fs::read_to_string("/proc/self/status").ok()?;
  for line in status.lines() {
    if let Some(rest) = line.strip_prefix("VmHWM:") {
      let kb: String = rest.trim().trim_end_matches("kB").trim().to_string();
      return kb.trim().parse().ok();
    }
  }
  None
}

#[cfg(not(target_os = "linux"))]
fn peak_hwm_kb() -> Option<u64> {
  None
}

/// Decode a source to normalized RGBA pixels the same way the encoder does
/// (orientation applied), for a like-for-like fidelity comparison.
#[cfg(feature = "bench")]
fn decode_source_rgba(source: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
  use image::ImageDecoder as _;
  let mut decoder = image::ImageReader::new(std::io::Cursor::new(source))
    .with_guessed_format()
    .ok()?
    .into_decoder()
    .ok()?;
  let orientation = decoder
    .orientation()
    .unwrap_or(image::metadata::Orientation::NoTransforms);
  let mut img = image::DynamicImage::from_decoder(decoder).ok()?;
  img.apply_orientation(orientation);
  let rgba = img.to_rgba8();
  let (w, h) = (rgba.width(), rgba.height());
  Some((rgba.into_raw(), w, h))
}

#[cfg(feature = "bench")]
fn decode_avif_rgba(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
  let img = image::load_from_memory(bytes).ok()?;
  let rgba = img.to_rgba8();
  let (w, h) = (rgba.width(), rgba.height());
  Some((rgba.into_raw(), w, h))
}

/// Alpha-aware fidelity: RGB error is weighted by each pixel's source alpha so
/// fully-transparent pixels (whose RGB is invisible and legitimately optimized
/// away) do not count. Returns (RGB PSNR dB, max alpha-channel error).
#[cfg(feature = "bench")]
fn alpha_weighted_psnr(a: &[u8], b: &[u8]) -> Option<(f64, u8)> {
  if a.len() != b.len() || a.len() % 4 != 0 {
    return None;
  }
  let mut weighted_err = 0f64;
  let mut weight_sum = 0f64;
  let mut max_alpha_err = 0u8;
  for (pa, pb) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
    let alpha = pa[3] as f64 / 255.0;
    max_alpha_err = max_alpha_err.max(pa[3].abs_diff(pb[3]));
    for c in 0..3 {
      let d = pa[c] as f64 - pb[c] as f64;
      weighted_err += d * d * alpha;
    }
    weight_sum += alpha;
  }
  if weight_sum == 0.0 {
    // Fully transparent image: only alpha fidelity matters.
    return Some((f64::INFINITY, max_alpha_err));
  }
  let mse = weighted_err / (weight_sum * 3.0);
  if mse == 0.0 {
    return Some((f64::INFINITY, max_alpha_err));
  }
  Some((10.0 * (255.0_f64 * 255.0 / mse).log10(), max_alpha_err))
}

fn main() {
  let items = corpus();
  let psnr_enabled = cfg!(feature = "bench");

  println!("| item | src KiB | avif KiB | saving | >=15% | encode ms | Δpeak MB | PSNR dB |");
  println!("|------|---------|----------|--------|-------|-----------|----------|---------|");

  let mut max_delta_kb: u64 = 0;
  let mut pass = 0usize;
  let mut total = 0usize;
  let mut total_ms = 0f64;
  let mut psnr_sum = 0f64;
  let mut psnr_n = 0usize;

  for item in &items {
    let src_kb = item.source.len() as f64 / 1024.0;
    let hwm_before = peak_hwm_kb();
    let start = Instant::now();
    let result = avif_encode::encode_image_to_avif(&item.source);
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    let hwm_after = peak_hwm_kb();

    match result {
      Ok(enc) => {
        total += 1;
        total_ms += ms;
        let avif_kb = enc.bytes.len() as f64 / 1024.0;
        let saving = 1.0 - (enc.bytes.len() as f64 / item.source.len() as f64);
        let ok =
          avif_encode::has_sufficient_saving(item.source.len() as u64, enc.bytes.len() as u64);
        if ok {
          pass += 1;
        }
        if let (Some(b), Some(a)) = (hwm_before, hwm_after) {
          max_delta_kb = max_delta_kb.max(a.saturating_sub(b));
        }

        #[cfg(feature = "bench")]
        let psnr_text = {
          let src = decode_source_rgba(&item.source);
          let dec = decode_avif_rgba(&enc.bytes);
          match (src, dec) {
            (Some((s, sw, sh)), Some((d, dw, dh))) if (sw, sh) == (dw, dh) => {
              match alpha_weighted_psnr(&s, &d) {
                Some((v, aerr)) => {
                  if v.is_finite() {
                    psnr_sum += v;
                    psnr_n += 1;
                    format!("{v:.1}/a{aerr}")
                  } else {
                    format!("inf/a{aerr}")
                  }
                }
                None => "n/a".to_string(),
              }
            }
            _ => "err".to_string(),
          }
        };
        #[cfg(not(feature = "bench"))]
        let psnr_text = "off".to_string();

        println!(
          "| {} | {:.0} | {:.0} | {:.0}% | {} | {:.1} | {:.1} | {} |",
          item.name,
          src_kb,
          avif_kb,
          saving * 100.0,
          if ok { "yes" } else { "no" },
          ms,
          max_delta_kb as f64 / 1024.0,
          psnr_text
        );
      }
      Err(rej) => {
        println!(
          "| {} | {:.0} | - | - | - | - | - | rejected:{:?} |",
          item.name, src_kb, rej
        );
      }
    }
  }

  println!();
  println!("**PSNR decoding enabled:** {}", psnr_enabled);
  println!("**Threshold pass:** {pass}/{total} items cleared the 15% saving bar");
  if total > 0 {
    println!("**Mean encode time:** {:.1} ms", total_ms / total as f64);
  }
  println!(
    "**Max peak-RSS delta:** {:.1} MB",
    max_delta_kb as f64 / 1024.0
  );
  if psnr_n > 0 {
    println!("**Mean PSNR:** {:.1} dB", psnr_sum / psnr_n as f64);
  }
}
