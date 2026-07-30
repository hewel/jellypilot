# Keep Library Image Cache entries in their origin encoding

_Status: Accepted. Supersedes ADR 0016 and amends ADRs 0013 and 0014._

JellyPilot will keep the SQLite-backed Library Image Cache and localhost image proxy, but it will cache only the response bytes and MIME received from the media server. The cache is format-transparent—including when a media server itself returns AVIF—and does not transcode, probe WebView codec support, switch active variants, or implement format-specific rejection and retry. Logical Library Image references, digest ETags with revalidation, server-scoped access, write-through streaming, request coalescing, reader-safe LRU eviction, the global byte budget, enable/disable bypass, status, and all-server Clear remain.

Real-time background AVIF encoding is removed because its CPU and memory behavior degraded the interactive Library Browser experience. Catalog schema version 4 therefore removes conversion and variant state and performs a one-time full reset of pre-version-4 cache bytes; preserving an origin-only cache is more important than preserving upgrade-time cached contents.
