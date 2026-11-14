# Validation Agent Policy

This document is the authority for verification scope, including the final pass. Select checks
that exercise the changed contract. Existing authorization for implementation includes these
checks; do not add an approval pause solely because the applicable tier is broader.

## Tiers

1. **Focused** — the default for localized edits. For Rust behavior, use tests in the touched
   crate (`bun run task rust test <crate>`), focused clippy (`bun run task rust clippy <crate>`),
   and `bun run task rust fmt --check`. For scripts, use the relevant Bun tests plus
   `bun run task typecheck`, `bun run task lint`, and `bun run task fmt --check` as applicable.
   Documentation and instruction-only changes need link, command, metadata, and consistency
   checks; they do not require application builds or tests. Use this tier when no cross-crate
   contract changes.
2. **Suite** — for contract or cross-crate changes: focused gates first, then `bun run check`
   (fmt + lint + scripts typecheck + workspace clippy) and `bun run task rust test` (workspace).
   Use when public types, crate interfaces, or more than one crate change.
3. **Native smoke** — for desktop-boundary crossings: add
   `xvfb-run -a bun run task iced run --smoke` (builds the app, renders one frame, exits) when
   acceptance crosses application startup, window/shell wiring, subscriptions, tray behavior, or
   configuration persistence. The smoke gate proves startup, not appearance — visual acceptance
   remains human (see root AGENTS.md).
   The standard smoke skips tray creation. GTK/tray startup changes also need a nonvisual
   probe that enables the real tray before embedded-host creation under a non-C `LC_ALL`,
   using an isolated D-Bus session and a usable native display. A tray-free smoke cannot verify
   that boundary; Xvfb without a presentation-capable Vulkan driver is not hardware evidence.

## Opt-in native regressions

The sole maintained joint entry is
`bun run task iced regress <tray|external|gpu|all> [--file <media>] [--out <report-dir>] [--hwdec <no|vaapi|vaapi-copy>] [--host-abi <legacy|3>] [--duration-secs <N>]`.
It is not part of normal startup or a replacement for the applicable project gates.
See [fork maintenance](../../README.md#fork-maintenance-and-joint-acceptance) for sync and
rollback, and [manual color comparison](../../README.md#manual-three-way-color-comparison)
for the separate human acceptance protocol.
`--host-abi` defaults to `legacy` (actual ABI v1/v2); select `3` explicitly for timeline
handoff acceptance. An ABI mismatch fails the run rather than counting fallback as success.
`--duration-secs` is optional and **gpu-only**, not permitted for `tray`, `external` or `all`.
Supply it once with ASCII decimal digits representing an integer from 1 through 3600;
signs, whitespace, fractions, exponent notation and non-ASCII digits are rejected.
Without it, the short regression is unchanged.

Prerequisites: Linux, `dbus-run-session`, the normal launcher build dependencies, and `xvfb-run`
for the External recovery scenario. Tray/GPU use the **current native display** and require
the pinned staged libmpv/baseline, an installed non-C locale and a presentation-capable Vulkan
adapter; GPU also requires `ffprobe`. The runner preserves the Wayland display socket (or X11
`DISPLAY`) and does not substitute Xvfb or a software driver for hardware acceptance.
Supply a real, seekable, moving local clip long enough for pause/seek/resume.
`gpu` and `all` reject missing media before preparation/build/startup; no implicit or synthetic
fixture is used. Private D-Bus sessions and temporary configuration/runtime directories isolate
app/IPC state while retaining the display connection. Probes manipulate only their own windows:
no desktop input injection, other-window control, visual screenshot inspection or display-setting
changes. If the current display/adapter cannot support the probe, record `unavailable`.
An isolated compositor must satisfy the same prerequisites: on Wayland, GPU acceptance
requires `zwp_idle_inhibit_manager_v1`; on X11 it requires XScreenSaver Suspend support.
A successful EGL connection alone does not establish Vulkan surface compatibility, frame
callback progress or idle inhibition. Missing protocols/adapters are not passing regressions.

The runner also preserves audio-session socket lookup separately from its private runtime:
`PIPEWIRE_RUNTIME_DIR` and `PULSE_RUNTIME_PATH` default to the original session locations,
while explicit audio routing variables (including `PIPEWIRE_REMOTE` and `PULSE_SERVER`)
are retained. Real audio acceptance must use a trusted baseline with a single explicit
backend, such as `ao=pipewire` without a trailing fallback comma. A mismatched or unavailable
`current-ao` then fails the GPU probe; use an audio-bearing fixture for this strict audio
acceptance mode. This does not verify physical speaker count or listening
quality; keep those as separate human acceptance.

- `tray`: creates the real GTK tray under a non-C locale before embedded-host initialization
  and renders through the actual compositor. Tray-free smoke cannot establish this result.
- `external`: confirms an explicit missing-embedded-asset rejection, then starts and exits
  External mode successfully with missing libmpv, baseline and Vulkan driver paths.
- `gpu`: first rejects a missing local file and then loads valid media using the same host.
  This tests ordinary playback-error recovery, not recovery from fatal acquire/error release:
  those stop the host session and require VO/host recreation, not stop/reload or ABI fallback.
  It exercises real decoded video, paused clock, seek/frame change, paused video-region
  resize through iced layout and real copy-target replacement (not OS window-resize acceptance),
  resumed playback, image work and in-memory readback through the actual compositor.
  It opens a second native surface, transfers the same host's decoded picture to it,
  returns to main, and closes the auxiliary surface. Readiness, advancing clock and
  nonuniform readback are required in both directions, with one host and two live
  surfaces during transfer. This probes the presentation API, not the production PiP
  coordinator, its controls, or window-manager placement.
  On Linux it also drives a real PlaybackSession through the production projection path:
  the session idle inhibitor binds on the live presentation surface when playback
  resumes, releases on pause, rebinds after last-window close/reopen, and releases on
  stop. It closes the last window, reopens through the shell/tray show route, checks
  the retained playback session/new renderer binding, acknowledges stop, then exits.
  This is nonvisual lifecycle evidence, not a color comparison or proof of a particular
  hardware decoder.
  `decoderSamples` separately records actual embedded `hwdec-current`, requested `hwdec`,
  input/output formats and dropped-frame counters. GPU-only `--hwdec` overrides exercise
  HostOptions after the baseline; unavailable properties remain explicit in the report.
  Samples also include requested/actual AO and `audio-params`/`audio-out-params`; a lifecycle
  pass with automatic backend selection is not evidence of a particular audio backend.

With `gpu --duration-secs N`, repeat that complete lifecycle on the same host, retaining
all existing assertions and normal teardown. Each cycle dwells in active playback for
`min(N, 30)` seconds. `N` is a minimum elapsed time from probe creation, not an exact
deadline for exit: only finish after a complete cycle at or after `N`, with at least one
completed cycle. Media must extend beyond the seek target (`min(duration / 2, 10)` seconds)
plus the playback dwell and a two-second reopen margin; short clips are explicitly rejected.
The native watchdog is `N + 120` seconds and the dispatcher child timeout is `N + 180`
seconds; the short defaults remain 120/180 seconds. These exclude dependency preparation
and compilation. The idle-inhibitor driver budget is 90 seconds per sustained cycle
(20 seconds short); each active-playback phase gets its dwell plus 20 seconds, while all
other phases get 20 seconds.
The active GPU phase has one absolute deadline, checked both during presentation and by
the existing 25ms `FrameTick`; loss of presentation callbacks does not suspend that check.
Last-window close/reopen remains under phase 5's budget. Completion, failure and final
teardown disarm the phase deadline. The idle driver still runs first with its original
budget: if it fails first, its original error is retained, followed by `GPU progress`
context (`phase`, `cycle`, `last_present_age`, presentation/copy counts and live surfaces).
Later deadline failures do not overwrite that first failure. These checks are cooperative,
not preemptive: synchronous work can delay a tick, and the phase budget is not armed before
the first presentation constructs the GPU probe; the existing idle/process limits still apply.
This is diagnostic boundedness, not recovery from missing frame callbacks or evidence
that sustained resource growth is resolved.
Loading waits for both the command acknowledgment and `file-loaded`; a retained frame from
the previous cycle is not evidence that replacement-media properties are ready. This wait
has a 20-second overall budget and retains the two-second IPC socket read/write timeout.

Default reports are `target/native-regression/report.json` and the selected scenario JSON files.
They include a fresh run identity, requested/actual host ABI and both shared/desktop Cargo
lockfile hashes; aggregate `pass` requires all requested scenarios to pass and their processes
to exit successfully. Failures, timeouts and unavailable prerequisites
must not be interpreted as pass. Check each item's status/reason and matching `runId`;
unselected files may be from an older run. Keep media/source/artifact metadata with the report.
Aggregate `requestedDurationSeconds` records the selected minimum or null for a short run.
Native optional/null `sustained` evidence contains `requestedSeconds`, `elapsedSeconds`
finalized at cycle completion and `completedCycles`. A sustained `pass` additionally
requires matching requested seconds, finite elapsed seconds at least the minimum and
at least one completed cycle; a default/short or stale report cannot satisfy it.
Native reports also retain `processId`, `hostsCreated`, `hostsDropped`, `hostIdentity`,
`phase`, `cycle`, timestamped `phaseHistory`, and cumulative probe `presentations`/`copiedFrames`
for same-host assertions and progress diagnosis. These counters do not include presentations
after the probe completes.
`lastPresentElapsedSeconds` records the last presentation accepted by the probe relative
to run startup, or null before one is observed; it remains available in failure/exit reports.
Phase codes are 0 load, 8 settle paused frame, 1 paused hold, 2 paused seek, 3 paused resize,
4 active playback, 10 auxiliary video, 11 return to main, 12 auxiliary close,
5 close/reopen, 6 advancing reopened playback, 7 stop/idle and 9 complete.
Phase history records transitions; resource samples correlated to it are not atomic phase
measurements and can miss short stages.
Resource metrics remain a separate read-only observer, not a new acceptance framework:
for example, attach `bun run task monitor --pid <processId> --out target/resources/sustained.ndjson`
while the process runs, and retain raw metrics/unavailability alongside run/PID/phase/cycle
evidence. Neither a lifecycle pass nor resource trends establish absence of leaks.
Human-authored `color-comparison.json` is separate and is never written or accepted by the
automatic entry. SDR, HDR10 and metadata-confirmed Profile 5 comparisons remain unavailable
until a human actually performs and records them.

## Diagnosing Smoke and Playback Failures

When the smoke gate or MPV playback fails, follow this route instead of ad-hoc spelunking:

- **App logging**: `JELLYPILOT_LOG` (tracing EnvFilter syntax) controls app diagnostics; default
  `warn`, output goes to **stderr** (`src-iced/src/runner.rs`). Typical values: `error`, `warn`,
  `info`, `debug`, `trace`, or module-scoped `jellypilot_iced=debug`. Re-run a failing gate as
  `JELLYPILOT_LOG=debug bun run task iced run --smoke`.
- **Stream anatomy**: the dispatcher pipes child output through, so cargo compile lines come first
  and app tracing starts after cargo's `Running …` line. Compile errors belong to cargo (they name
  the crate and file); failures after `Running` belong to the app.
- **Failure segment**: a failing smoke gate ends with an `=== iced smoke [FAILED] ===` segment
  (command, exit code, hint). Absence of that segment means the failure happened before the app
  started — inspect the cargo/tooling output above it.
- **Player logs**: in Settings → Diagnostics, enable **Capture player logs**, reproduce, then
  export logs. This session-only opt-in subscribes to the active embedded or external MPV's
  native `log-message` events at `v` level over JSON IPC. Pre-enabled capture waits for
  the subscription response before returning a new IPC connection; later toggle changes
  are checked every 250 ms. Check the `Player log subscription active` marker in the export. It captures track refresh/seek and audio-start messages plus selected playback
  commands, outside the GPU queue lock, without periodic media-property queries. Capture
  starts at subscription, so earlier initialization messages are unavailable. Closing capture
  unsubscribes and retains its sanitized 8 MiB tail; restarting clears it. The independent
  `Player log` export section includes relative millisecond timestamps, connection IDs and
  explicit loss markers; detailed messages do not consume the 200-row Diagnostics list.
  URL/credential-bearing messages are omitted before capture. This is separate from
  `JELLYPILOT_LOG`, which controls application tracing. Exports use fresh files even within
  the same second, preserving previous evidence.
- **Playback errors**: a missing external binary surfaces as `MPV executable not found`;
  External MPV also retains its stderr output. App-side IPC errors are visible at
  `JELLYPILOT_LOG=debug`. Player log subscription failure is recorded in the export; an
  enabled toggle alone does not prove that the native subscription succeeded.

## Rules

- Test observable behavior, meaningful boundaries, and realistic regressions. Do not add
  case-by-case tests for simple command forwarding, constants, defaults, or field copying.
- Assert error semantics (rejection, error type/code, or relevant structured context), not
  incidental error-message wording. Assert wording only when it is an explicit required contract.
- Do not re-run suites the change cannot affect: no workspace clippy for a `scripts/**`-only edit,
  no smoke gate for display-free logic in `jellypilot-core`.
- Prefer cached incremental reruns. A focused `bun run task rust test <crate>` after a warm build is
  seconds; reserve full gates for the final pass of multi-step work, not for every intermediate
  edit.
- A failing, timed-out, or cancelled verification run is not a pass. Investigate the failure and
  rerun after a relevant fix or changed prerequisite. If an external prerequisite prevents
  verification, report the exact blocker and completed checks; do not retry unchanged failures indefinitely.
- When a change spans tiers, verify each completed step at its own tier and the finished work at
  the highest applicable tier. Reuse passing results for unchanged inputs; repeat or broaden
  checks only after relevant changes, failures, or unresolved concerns.
- If the user interrupts a verification run as excessive, drop to the lowest tier that still
  exercises the changed contract and state what was and was not verified.
