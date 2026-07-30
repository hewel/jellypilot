# Library Image optimization — release qualification (#195)

Status of the release-qualification gate for the Library Image cache and AVIF
optimization epic (#187–#194). Each criterion is mapped to its proof.

## Cross-platform compile

- **PR-gated compile matrix.** `.github/workflows/ci.yml` adds a
  `compile-backend` job (matrix: `ubuntu-latest`, `windows-latest`,
  `macos-latest`) that runs on every pull request — it no longer waits for a
  release tag. It compiles the backend with the **production** feature set
  (`cargo check --locked`, default features = SQLite catalog + `image`/`ravif`
  codec), deliberately **not** `--all-features`, so the dav1d benchmark decoder
  and the webdriver harness stay out of the production gate.
- **Linux full lane.** The existing `check` job (ubuntu) retains lint
  (`bun run check` incl. `cargo clippy --all-targets --all-features`), the full
  unit/behavior suite (`bun run test:all`), the frontend production build, and
  the focused native E2E path (`bun run e2e:verify`). Windows/macOS legs are
  compile-only, matching the "compile-only unless stable native runners exist"
  allowance.

## Behavioral proofs (real-listener integration + native E2E)

The proxy-path proofs run as **real-listener** Rust integration tests — an
actual TCP image-proxy listener plus a mock origin — which is the native proxy
path end to end. The mounted-app smoke (`image-proxy-service.e2e.ts`) was
rebuilt and passes (`EXIT=0`) with all epic changes, confirming the app boots
and the proxy serves through typed IPC.

| #195 behavior | Proof |
|---|---|
| Uncached origin streams before durable commit | `image_proxy::tests::test_cache_miss_then_hit_serves_from_disk` — miss streams from origin once, hit serves from disk with zero additional origin hits |
| Completed original is a hit after restart | `image_cache::tests::first_miss_commits_and_restart_hits` — re-init over the same root hits |
| AVIF capability gating + accepted activation via the same logical URL, changed MIME/ETag, conditional revalidation | `image_proxy::tests::test_etag_revalidation_304_and_stale_validator` (200/304/stale-200, strong representation ETag), `test_supported_capability_serves_cached_avif` (`image/avif` served from cache), `test_unsupported_capability_refetches_avif_only_entry` (unsupported → origin re-fetch) |
| Unsupported capability restores/re-fetches origin with one bounded frontend retry | `test_unsupported_capability_refetches_avif_only_entry` (proxy), `tests/video-card.test.tsx` one-shot retry then fallback (WebView), `image_cache::tests::reject_avif_*` (reject restores origin + terminal failed) |
| Transient retry timing | `image_cache::tests::retry_schedule_is_10s_1m_10m_and_terminal_after_four` |
| Crash-orphan adoption | `image_cache::tests::recover_adopts_valid_orphan_avif_and_deletes_invalid` |
| Stale-temp cleanup | `image_cache::tests::recover_removes_stale_temp_but_keeps_locked_temp` |
| Policy-version requeue | `image_cache::tests::recover_requeues_old_policy_terminal_but_keeps_active_avif` |
| Cross-process worker ownership / wakeup | `avif_worker::tests::disabled_cache_releases_lock_and_recovers_on_reacquire`, `worker_shutdown_is_prompt` |
| Active-reader-safe deletion | `image_cache::tests::active_reader_is_not_evicted`, `clear_removes_unpinned_and_defers_pinned` |
| Disabled bypass with retained usage | `image_proxy::tests::test_disabled_cache_bypasses_reads_and_writes`, `tests/library-settings-card.test.tsx` retained-disabled copy |
| Confirmed all-server Clear without stale writers/encoders repopulating | `image_cache::tests::clear_removes_unpinned_and_defers_pinned`, `clear_blocks_stale_writer_and_resumes_caching`, `epoch_guard_blocks_stale_*` |
| Corrupt-catalog startup quarantines/rebuilds while images continue through origin | `image_cache::tests::corrupt_catalog_is_quarantined_and_rebuilt` (fail-open: catalog is rebuilt, serving falls back to origin) |

## Benchmark constants

`CONVERSION_POLICY_VERSION` is unchanged at `1`. The codec constants match
`docs/avif-policy-benchmark.md` exactly: color quality 80, speed 8, lossless
alpha (100), 15% saving threshold, 32 MiB / 24 Mpx / 12000 px admission limits.

## No legacy references

No references remain in the cache/proxy path to JSON image indexes, offline
cache hits, write-only toggle semantics, or immutable representation responses
(grep-verified; the only `immutable` hit is unrelated auth-profile wording).

## Remaining work (explicitly not done here)

The literal **mounted-WebView** Library Image E2E specs — driving the compiled
app's WebView to load artwork through the proxy and asserting
stream-then-commit, restart-hit, AVIF activation, and rejection recovery end to
end — are not added. They require (a) expanding the harness `SafeRealCommand`
boundary (currently only `app_local_services` and `config_default`) so the
backend can connect to a controlled media-server origin for real, and (b) a
mock origin fixture. That boundary expansion is a deliberate, reviewable step
per `docs/agents/e2e.md` and is the single outstanding piece of this gate. The
behaviors those specs would assert are already covered at the real-listener and
jsdom layers enumerated above.
