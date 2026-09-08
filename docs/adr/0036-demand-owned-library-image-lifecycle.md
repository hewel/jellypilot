# Own Library Image demand at each rendered location

_Status: Accepted in the 2026-09-08 architecture discussion. Amends ADR 0028's page-granular load plans and slot-keyed handles, and ADR 0029's shared binder/handle ownership. Original-byte disk caching, adapter-owned raster decoding, and External MPV Playback remain unchanged._

Pages own a native `ImageCollection` of currently demanded locations instead of coordinating a global binder, page load plans, completion checks, and handle retention. The display-free `jellypilot-core::image_lifecycle` module decides location identity, priority transitions, removal, and whether a completion still belongs to the current attempt. The iced collection owns each demand control, abortable completion task, and local display handles. Process-unique attempt tokens reject completions after replacement, departure, or recreation; collection epochs reject geometry from discarded pages or sessions. Retaining unchanged metadata does not revoke overlapping demand or invalidate its final geometry observation.

## Measured demand, not page-wide loading

Visible cold images are admitted immediately, with authorized raster-cache hits applied synchronously. Prefetch extends one actual viewport before and after along the scrolling axis, clipped to content and ancestor scroll containers. There is no loading debounce, velocity prediction, or preload of unselected full-size hero images. Page code continues to choose the image, including Episode Still fallbacks and Title Logo shadow variants; it does not schedule image work.

A root widget operation measures image markers after input handling, including initial redraw, nested horizontal rails, and vertical scrolling. Root-owned observation history also revokes locations whose entire widget subtree disappears or temporarily stops participating in traversal; returning markers publish demand again. Browse measures its actual grid-local viewport below the count header, preserving the full parent height and signed offset. Its metadata and widget windows remain sparse and cover the measured prefetch range without constructing the full library on scroll.

## Shared work has independent consumers

`ArtworkAdapter::demand` returns either an authorized immediate settlement or an independent RAII control and completion receiver. The existing asynchronous load methods use this same registry. A concrete adapter-owned worker performs each unique image/size/derived-variant request; dropping its initiating consumer does not cancel another consumer. The last control cancels waiting or interruptible network work. A distinct attempt identity prevents a late worker or old control from affecting a replacement with the same cache key.

Admission selects visible demand before prefetch, with live promotion and demotion. Waiting demand occupies only current consumer metadata; it does not reserve network/decode capacity or fail because an unrelated page filled a fixed batch queue. The registry replaces the old queue protocol rather than layering another scheduler over it. Active-load and aggregate-byte budgets, response/decode guards, and the existing encoded/raster LRUs remain authoritative. A noninterruptible blocking decode retains its permit until the blocking work ends, even after its consumers leave.

## Resource and session boundaries

A prefetched completion leaves pixels only in the adapter's budgeted raster cache, not an extra native cache or eagerly created display handle. Promotion re-enters the authorized demand path, rebuilding a handle from the cache or loading again if that raster was evicted. Demotion drops native display handles. Navigation revokes only the departing page's demand; Now Playing remains independent while any of its images are needed. Hidden windows suspend image demand, and showing a window restores its selected images. Disconnect and profile handoff invalidate the session; Control-Only drops the Full-mode composition without stealing Now Playing's work.

Diagnostics still report sanitized aggregates rather than per-image rows. A short one-shot counter flush batches a burst of settlements; it never delays image admission or rendering. No generic adapter trait, second pixel cache, reverse media-server-to-core dependency, or second styling mechanism is introduced.
