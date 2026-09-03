# Freeze Linux Control-Only idle resource budgets

_Status: Accepted. Executes the resource gate defined in ADR 0030._

ADR 0030 requires a measured Linux baseline before any Control-Only restructuring is evaluated. This ADR freezes the baseline, the comparison protocol, and the resulting budgets that the same-process `Runtime` + `FullUi`/`ControlUi` candidate must satisfy.

## Protocol

Every measurement under this gate follows one executable protocol:

- **Repetition count is fixed at three valid runs** for both baseline and candidate. There is no role-specific count.
- **State labels are durable.** Each run is recorded with `bun run task monitor --pid <pid> --out <file> --label "<state>"`, and the report serializes that label. The label names the app mode, window state, and playback state. (Exception: the baseline runs recorded on 2026-09-03 before the `--label` flag existed carry no label; their timestamps and PIDs are mapped to conditions in this ADR, which is the durable record for those legacy runs.)
- **Preconditions.** Signed in, no active Playback Session, target window state reached, then a five-minute stabilization period before sampling starts.
- **Observation.** 301 samples at 1 Hz, covering t=0 through t=300000 ms.
- **Per-run aggregation.** Memory metrics (RSS, PSS, GPU resident) use the run median. Rate metrics (CPU time, context switches, GPU engine time) use the run's computed per-second rate.
- **Valid-run rule.** A repetition is invalid if its RSS within-run spread exceeds 5% (`(max − median) / median > 0.05`). Invalid runs are discarded and replaced; they are never averaged in.
- **Cross-run aggregation.** The baseline takes the maximum per-metric value across the three valid baseline repetitions. A candidate takes the mean across its three valid repetitions and is compared directly against each budget; there is no additional noise relaxation, because taking the baseline maximum already absorbs upward drift and the visible-state budgets carry a further 5% tolerance.
- **Rounding.** Budgets expressed as integers are rounded up to the next integer (ceiling).

## Baseline

Host: Linux, niri (Wayland), AMD Radeon RX 7900 XTX. App: `target/release/jellypilot`, signed in, Control-Only Now Playing window visible, no playback. Evidence: `target/resources/baseline-visible.ndjson` (gitignored; the numbers below are the durable record).

Three valid repetitions (per-run medians; rates as recorded):

| Metric | Run 2 (01:17Z) | Run 3 (01:31Z) | Run 5 (01:58Z) | Baseline (max) |
| --- | --- | --- | --- | --- |
| RSS | 464,484 KiB | 464,488 KiB | 446,056 KiB | 464,488 KiB |
| PSS | 414,927 KiB | 414,841 KiB | 391,913 KiB | 414,927 KiB |
| CPU time | 0.033 ms/s | 0.100 ms/s | 0.000 ms/s | 0.100 ms/s |
| Context switches | 1.59/s | 5.00/s | 0.20/s | 5.00/s |
| GPU resident | 263,884,800 B | 263,884,800 B | 361,213,952 B | 361,213,952 B |
| GPU engine time | 11.4 µs/s | 15.6 µs/s | 0 µs/s | 15.6 µs/s |
All timestamps are 2026-09-03 UTC. Runs 2, 3, and 5 all measured PID 57973 (the same process); the excluded run 1 measured the earlier PID 30737.

**Observed run-to-run drift (valid runs).** RSS 4.0%, PSS 5.5%, GPU resident 36.9% (263,884,800 → 361,213,952 B, about 251.7 → 344.5 MiB, within the same process between 01:31Z and 01:58Z and stable within each run), CPU/context/engine differences small in absolute terms. The GPU drift shows the idle footprint depends on session history even in valid idle runs; the max-based baseline plus mean-based candidate comparison absorbs this without a separate noise allowance.

**Excluded repetitions.** Run 1 (01:01Z, first process) and run 4 (01:52Z, PID 57973) are invalid under the valid-run rule: within-run RSS spread 19.8% and 14.8% respectively (vs ≤0.01% in all valid runs), indicating the idle precondition was not held for the full observation. Both were replaced, per protocol. Separately, manual observation during run 1 showed library artwork still loading; that observation is not part of the NDJSON evidence and plays no role in either exclusion.

## Frozen budgets

**Visible Control-Only window (candidate must not regress meaningfully):**

| Metric | Budget | Derivation |
| --- | --- | --- |
| RSS | ≤ 487,713 KiB | 464,488 × 1.05 |
| PSS | ≤ 435,674 KiB | 414,927 × 1.05 |
| GPU resident | ≤ 379,274,650 B | 361,213,952 × 1.05 |
| CPU time | ≤ 1.0 ms/s | 10× baseline, ≈0.1% of one core |
| Context switches | ≤ 10/s | 2× baseline |
| Tray Show → interactive first frame | ≤ 1 s | ADR 0030 reopen limit |

**Zero-window daemon (the improvement ADR 0030 exists to prove):**

| Metric | Budget | Derivation |
| --- | --- | --- |
| GPU resident | ≤ 36,121,396 B | 361,213,952 × 0.10; destroying the window must release the wgpu surface |
| RSS | ≤ 334,432 KiB | 464,488 × 0.72; renderer/glyph/font state must shed |
| PSS | ≤ 298,748 KiB | 414,927 × 0.72 |
| CPU time | ≤ 1.0 ms/s | no display work exists without a window |
| Context switches | ≤ 5/s | remote-session and tray wakeups only |

## Gate outcome (2026-09-03)

The candidate was measured once per state before a defect fix, then accepted by the maintainer without a full post-fix re-run. Recorded evidence: zero-window runs passed GPU (no attributable GPU memory — the wgpu surface is destroyed with the window), RSS (mean 81,485 KiB), PSS (mean 49,442 KiB), and CPU (0.53 ms/s); valid visible runs passed RSS (365,060–365,496 KiB), PSS (333,804–337,182 KiB), GPU (275,656,704–326,037,504 B), and CPU (≤1.23 ms/s). Both states failed the context-switch budget (~41/s) due to the single-instance accept loop polling at 25 ms; the fix (blocking accept with self-connection wake-up) was verified empirically at 0 ctx/s on an isolated headless instance, but the full protocol was not re-run afterward. The verdict is therefore **accepted with partial evidence**: every budget except context switches is measured-pass, and context switches are fixed-with-spot-check rather than protocol-pass. If idle power use ever becomes a concern, re-run the two-state protocol to close this gap.

## Wayland observation (manual, not NDJSON evidence)

While preparing the baseline, close-to-tray was exercised manually on niri: `niri msg action close-window --id <id>` was delivered to the JellyPilot window, the process took the tray branch and stayed alive, but the window remained fully mapped with unchanged layout according to `niri msg windows`. This is an uncaptured manual observation, not part of the sampler evidence; it indicates the current `Mode::Hidden` close path does not hide the window on this Wayland session. It is consistent with ADR 0030's direction — the zero-window daemon lifecycle is the only path to a real background state on Wayland — but it does not by itself prove the winit/Wayland mechanism, which remains to be confirmed during implementation.

## Consequences

- The same-process candidate is evaluated against these numbers; missing any frozen budget reopens the process-boundary comparison per ADR 0030.
- Baseline conditions and run-to-condition mapping are recorded here so the gate survives cleanup of the gitignored NDJSON artifacts; future baseline or candidate runs must carry `--label`.
- Windows/macOS budgets are intentionally absent until their own baselines exist.
