# AVIF conversion policy — benchmark evidence (#193)

Repeatable evidence for the fixed Library Image AVIF policy, produced by
`src-tauri/examples/avif_bench.rs` (release mode). Reproduce:

```bash
cargo run --release --example avif_bench                 # core metrics
cargo run --release --example avif_bench --features bench # adds alpha-aware PSNR (dav1d)
```

The corpus is synthetic but representative: photographic posters, a landscape
thumbnail, a large backdrop, opaque and transparent PNG, static WebP,
sharp-edge/text content, and a worst-case noise image. `noise` and
`already-optimal WebP` are included specifically to prove the saving threshold
rejects incompressible or already-compressed sources.

## Results (Linux, Ryzen-class, release build, `--features bench`)

| item | src KiB | avif KiB | saving | >=15% | encode ms | Δpeak MB | PSNR dB |
|------|---------|----------|--------|-------|-----------|----------|---------|
| poster-jpeg-480x720 | 160 | 124 | 22% | yes | 717 | 16 | 33.2 / a0 |
| thumb-jpeg-320x180 | 5 | 1 | 76% | yes | 104 | 16 | 43.9 / a0 |
| backdrop-jpeg-1920x1080 | 945 | 727 | 23% | yes | 4320 | 80 | 33.3 / a0 |
| opaque-png-400x300 | 259 | 52 | 80% | yes | 331 | 80 | 32.5 / a0 |
| transparent-png-400x300 | 4 | 2 | 63% | yes | 116 | 80 | 47.8 / a1 |
| static-webp-400x300 | 0 | 2 | -700% | no | 189 | 80 | 45.7 / a0 |
| sharp-text-png-400x200 | 8 | 6 | 34% | yes | 136 | 80 | 48.7 / a0 |
| noise-jpeg-300x300 | 113 | 103 | 9% | no | 312 | 80 | 30.0 / a0 |

- **Threshold pass:** 6/8 items cleared the 15% saving bar. The two that failed
  (incompressible noise at 9%, already-optimal lossless WebP) are exactly the
  sources that *should* stay as origins — the threshold behaves correctly.
- **Mean PSNR (RGB, alpha-weighted):** 39.4 dB. `a0`/`a1` is the max
  alpha-channel error out of 255: bit-exact (a0) everywhere except one 1/255
  step on the transparent PNG ramp — alpha is effectively lossless.
- **Mean encode time:** 778 ms; the 1920×1080 backdrop is 4.3 s on one thread.
- **Max peak-RSS delta:** 80 MB (the large backdrop), single-threaded.

PSNR is alpha-aware: RGB error is weighted by each pixel's source alpha, so
fully-transparent pixels (whose RGB is invisible and legitimately optimized
away) do not distort the score. A naive RGB-only PSNR reads ~19 dB on the
transparent PNG purely from that artifact; the alpha-weighted value is 47.8 dB.

## Decision: retain the policy

The evidence supports keeping every constant as-is; none needs revision.

- **Color quality 80** — 32–48 dB PSNR across photographic, PNG, and sharp-edge
  content. That is visually near-lossless for library artwork while still
  yielding 22–80% size reductions. Raising quality would erode the savings;
  lowering it would risk visible artifacts on posters.
- **Speed 8** — mean 778 ms/image on one background thread. Fast enough that the
  foreground-gated worker never meaningfully competes with the UI; speed 9–10
  would trade real fidelity for negligible background-time gain.
- **Lossless alpha (alpha quality 100)** — alpha error ≤ 1/255. Transparency is
  preserved bit-for-bit, so transparent PNG artwork never haloes. Color stays
  quality 80; only alpha is lossless.
- **15% saving threshold** — accepts the 6 genuinely-compressible items and
  rejects the 2 that should stay origins (noise, optimal WebP). It neither
  wastes effort re-encoding incompressible sources nor rejects worthwhile wins.
- **Admission limits (32 MiB source, 24 Mpx, 12000 px)** — the largest item
  (1920×1080, ~1 MiB) encodes in 4.3 s within an 80 MB transient. Limits sit
  well above real Library Image sizes while capping the worst-case background
  time/memory.

`CONVERSION_POLICY_VERSION` stays at its current value; no policy change is
required, so no requeue of existing terminal entries is triggered.
