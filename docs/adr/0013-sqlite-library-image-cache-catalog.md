# Use SQLite for the Library Image cache catalog

_Amended by [ADR 0017](0017-origin-encoded-library-image-cache.md): SQLite remains the catalog, but conversion jobs, variants, and retries no longer exist._

JellyPilot will use one SQLite catalog as the source of truth for Library Image cache entries, active file variants, AVIF conversion jobs, retries, and restart recovery. This replaces the planned per-server `index.json` catalog rather than layering a separate job database beside it: atomic claims and state transitions outweigh the added database dependency and migration ownership, while one catalog avoids split-brain metadata.
