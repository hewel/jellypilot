# Playback Feature Delivery

Accepted on 2026-10-05. The user authorized implementation in the order below,
design decisions, and coherent local commits. This document separates accepted
scope from delivery and human acceptance; it does not replace the existing
playback, account, or platform contracts.

## Delivery order

| Batch | Accepted scope | Delivery status |
| --- | --- | --- |
| 1 | Desktop seek thumbnails and Linux MPRIS | Implemented; automated checks and independent review passed; human acceptance pending |
| 1, addition | Progressive blur for embedded player controls | Compatible iced candidate in verification; app integration pending |
| 2 | Android controls an authorized living-room PC/TV Playback Target through the media server | Not started |
| 3 | Editable, temporary viewing queue containing movies and episodes within one Profile Scope | Not started |
| 4 | Secondary text subtitles and A/B loop for the active Playback Session | Not started |
| 5 | Named, device-local saved browse filters within one Profile Scope | Not started |

Later batches must use the established server and playback adapters. They do not
authorize a new pairing service, background Android playback, Android TV, or a
cross-server queue. Saved filters remain independent of Favorites and Watchlist.
Implementation-specific contracts and evidence will be recorded as each batch
is completed.

## Seek Preview

The first implementation targets the Desktop embedded player's interactive
timeline. Hovering or dragging may display an existing Jellyfin Trickplay image
for the current item and actual media source. It does not decode video or ask
the server to generate new images. Missing metadata, unsupported providers,
loading, and failed requests retain the existing time and real chapter labels.

Preview work is scoped to the authenticated connection and current playback
identity. A replaced item, source, or profile must never display an old image.
Requests, decoded dimensions, response sizes, and memory retention are bounded.
The preview must not contain or log access credentials.

The image fits within 240×135 logical pixels and the measured timeline width,
preserving its source aspect ratio. A 12px corner radius, 8px tooltip padding and
8px image-to-text gap retain the existing 12px monospace time. Real chapter text
may wrap and increase the tooltip height; 256×179 is only a short-caption 16:9
Paper reference. The existing viewport clamp remains authoritative. Loading,
missing-image and failed-image states all use the same time/chapter fallback,
and an earlier candidate image must not be substituted for the current one.

Hovering never seeks. Drag release retains the existing single committed seek;
fullscreen transitions and playback replacement continue to cancel stale drafts.
The tooltip stays within the viewport and its picture does not obscure its time.
Existing 40px seek targets and 44px main transport controls remain unchanged.
Android and TV thumbnail interfaces are not included in this first batch.

## Linux system media controls

MPRIS exposes the current Playback Session through the session D-Bus. Available
transport controls, position, rate and seeking capabilities reflect the actual
player; unsupported capabilities are not advertised. Metadata excludes
credential-bearing media and artwork URLs.

Commands are revalidated against the active authenticated connection and
playback generation before execution. SetPosition additionally checks the track
identity. Profile handoff, stale queued commands, and replacement playback must
not control a different item. Explicit resume of hidden embedded playback must
restore its window through the existing lifecycle route before playing.

MPRIS is Linux-only. A missing session bus must not prevent application startup
or playback. It introduces no second transport state machine or busy polling.
The initial service does not promise MPRIS TrackList, Playlists or OpenUri.

Next/Previous are advertised only while playing and a real adjacent episode is
available; paused calls cannot accidentally resume playback. Observed rate remains
accurate even when an external MPV configuration exceeds the app's 0.25–4 range.
The declared range includes that observed rate; new settings use MPRIS best-fit
semantics within the app's supported range. Successful seek settlement emits
Seeked only once, and rejected or stale receipts cannot produce a new signal.

A missing bus, another owner of the service name, or a lost bus connection
disables this integration until application restart. Name acquisition neither
replaces another instance nor queues behind it. The startup smoke path omits the
live bus integration; protocol tests use an isolated D-Bus service and client.

## Embedded player progressive blur

The user's new direction supersedes the earlier temporary no-blur exception for
the embedded player's Full controls. Blur should progressively sample the live
video scene behind the bottom controls, leaving the upper picture, controls,
and Seek Preview sharp. Blurring a poster is not equivalent. MPV-rendered
subtitles may already be composited into the video texture; the backdrop cannot
independently exclude those pixels. Subtitle legibility within the effect region
therefore needs human acceptance, not a promise of a separate sharp subtitle plane.

Minimal/Full states, independent top-corner controls and Floating Popovers,
focus/drag retention, input semantics, reduced-motion usability, and the FP16
embedded composition remain intact. Hidden control surfaces must not leave a
blur effect running. The docked browser footer and external Control-Only screen
are outside this visual change.

The iced dependency must supply a compatible, reproducible implementation before
the app enables the effect. Final radius, falloff, tint and crop are implementation
parameters to validate at that boundary; a Paper image cannot prove live-frame,
HDR, performance, or lifecycle correctness.

## Verification and acceptance

Follow [the validation policy](agents/validation.md): focused behavior tests and
clippy first; cross-crate changes require the suite tier, and subscription/window
wiring also requires native smoke. Review must specifically check stale playback
commands, profile isolation, bounded thumbnail decoding, and hidden-window
resume. Native appearance, actual media/control integration, display color and
device performance remain human acceptance; neither Paper nor smoke proves them.

Preserve unrelated Android and other local changes. Commit completed slices
with explicit paths; no push is authorized by this delivery plan.

### First-batch evidence

The preview and MPRIS additions passed `bun run check`, workspace Rust tests
(1519 tests, one existing ignored test; the D-Bus test also executes once in a
private child process), and native startup smoke. After the final MPRIS receipt
and signal refinements, the affected iced checks passed again: 624 tests
(163 UI, 10 embedded-host, 451 app), focused clippy and format checks.

Independent preview review found no Standards or Spec issues. MPRIS review found
small-seek notification, paused next/previous, and rate-range inconsistencies;
all were fixed. Follow-up review also caught duplicate settlement notification,
which now records only an actually applied controller effect and has a replay
regression. Final review has no remaining findings.

The first-batch Paper handoff is `/tmp/media-streamer-player-features-handoff.md`:
nine new Desktop boards, 39 total, finish returned OK with token hash `28f17849`.
Its [contract board](https://app.paper.design/file/01M1XG9QCM2M58ENY2ZVA2YTWA/1-0/QMK-0)
records the adaptive preview and provisional blur material. Paper completion is
separate from the still-pending progressive-blur app integration and human checks.
