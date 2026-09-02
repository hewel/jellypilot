# Stream and decode Library Images behind the artwork seam

_Status: Accepted. Complements [ADR 0017](0017-origin-encoded-library-image-cache.md) and amends the loading half of [ADR 0027](0027-cross-platform-iced-frontend.md) slice 5._

## Context

The iced frontend's Library Image pipeline (ADR 0027 slice 5) fetches through a
well-formed adapter — in-memory LRU, request coalescing, bounded scheduler, disk cache —
but three structural choices make Library Images load slowly:

- **Whole-page `join_all` batches.** Each surface (Home, Browse, Detail) builds one
  batched task over every image on the page; the page paints no image until the slowest
  of ~24 settles. The pipeline is also written three times in `src-iced/src/app/update.rs`.
- **Decode leaks across the seam.** `RawArtworkDecoder` validates container bytes and
  returns them unchanged, so pixel decode falls to iced's image pipeline: a single
  background worker thread, FIFO, keyed by handle id. Because handles are slot-keyed and
  dropped on every view leave, every back-navigation re-decodes every image through that
  one thread, at the full 600px request size, for cards displayed at ~160–200px.
- **No visibility awareness, no telemetry.** The scheduler is strict FIFO, scrolling never
  triggers or reprioritizes loads, and `DiagnosticCategory::Artwork` records nothing, so
  sizing decisions have had no data.

## Decision

**Add the Library Image Raster as a first-class concept.** A Library Image Raster is an
in-memory, display-sized RGBA decode of a Library Image, keyed by image reference and a
size class (`Card`, `Hero`, `Backdrop`). Rasters accelerate first paint and repeat
rendering; they are never persisted and are distinct from the Library Image Cache, which
continues to store only origin-encoded bytes (ADR 0017 unchanged). Size classes are
render-side decode buckets, not new logical reference kinds; ADR 0014's reference model
and the current request sizing (`maxWidth=600/1920, quality=90`) are unchanged until
telemetry justifies revisiting them.

**Move decode behind the adapter seam.** `ArtworkAdapter` decodes and downsamples to the
requested size class on its existing blocking-pool step and stores rasters in a
byte-budgeted LRU. The `ArtworkDecoder` trait — a one-adapter hypothetical seam whose
only implementation was a pass-through — is deleted; `load` returns a concrete raster.
Decode applies EXIF orientation as iced's loader does and keeps the animated-container
rejection. Callers receive render-ready pixels and build `image::Handle::from_rgba`,
iced's synchronous handle path, eliminating the worker round-trip and per-handle-id
re-decode.

**Stream per-image completions from one loading module.** The three per-surface batch
pipelines in `update.rs` collapse into one display-free orchestration module in
`jellypilot-core` beside `artwork_binder`: given a surface's settled items it produces a
visibility-ordered load plan and per-image settlements. `src-iced` executes the plan as a
`Task::run` stream emitting per-image `ArtworkLoaded` messages (replacing
`ArtworkBatchLoaded`), so each card paints as its own image settles.

**Schedule visible-first.** `LoadScheduler` admission gains a lane (`Visible` /
`Offscreen`); the visible lane drains first, so a new page's visible images preempt queued
offscreen work. Loading stays page-granular: scrolling remains `Task::none()` and
viewport-sliced loading is not introduced.

**Instrument at the seam.** Each load carries a tracing span (duration, byte size, source:
memory/disk/network); aggregate counts and durations are recorded under
`DiagnosticCategory::Artwork` at support-view granularity — no per-image events, no URLs
or identifiers, per the Diagnostics domain contract.

**Land in two slices.** (i) Streaming loading module + two-lane scheduler + telemetry
skeleton — the immediate felt win. (ii) Adapter decode + size-class raster LRU +
`from_rgba`. Each slice lands green independently.

## Consequences

- The deletion test removes `ArtworkDecoder`/`RawArtworkDecoder`; decode policy
  (threading, sizing, budget, eviction) gains locality in `ArtworkAdapter`, and every
  surface gains leverage through the unchanged `load` interface.
- Orchestration is display-free and tested in `jellypilot-core` per ADR 0027's
  verification model; the adapter is tested through its external seam (feed bytes, assert
  rasters) — replace, don't layer; tests on the deleted pass-through are removed.
- Back-navigation cost drops to raster-LRU lookup + `from_rgba` handle rebuild; handles
  stay slot-keyed so there is exactly one pixel cache.
- Rejected and recorded to prevent re-litigation: identity-keyed handle LRU (a second
  pixel cache; rebuild from the raster LRU is cheap); viewport-sliced or scroll-triggered
  loading (page-granular with visible-first suffices; revisit with telemetry); per-image
  diagnostics events (flood the user-facing Diagnostics view); keeping `ArtworkDecoder`
  as a test seam (single adapter; the external seam is the test surface).
- If telemetry later shows transfer bytes dominate decode, request sizing is revisited as
  an amendment to this ADR, not as a silent constant change.
