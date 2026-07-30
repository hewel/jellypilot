# Keep Library Image references logical and revalidated

_Amended by [ADR 0017](0017-origin-encoded-library-image-cache.md): logical references and revalidation remain, but JellyPilot no longer switches locally generated encodings._

A Library Image reference identifies one version of the server image, not one local byte encoding; JellyPilot may therefore serve the original representation first and AVIF later from the same reference. The localhost image proxy will use active-variant ETags and revalidation instead of long-lived `immutable` responses, avoiding frontend invalidation machinery while allowing later loads to adopt a completed optimization.
