# 本地视频实验 / Local video experiment

Standalone Linux-first demo of the reusable `jellypilot-player` library: GStreamer decodes a local video and synchronized audio; a CPU-readable RGBA sample is uploaded through the pinned iced fork's custom shader seam. The demo remains disposable. Library reuse is **not** a production Playback Session or approval to embed playback in JellyPilot. Production continues to use External MPV Playback.

## Isolation and decisions

- One isolated workspace rooted at `crates/jellypilot-player/Cargo.toml`, containing the `jellypilot-player` library and the `jellypilot-local-video` demo at `experiments/iced-local-video`. Its independent lockfile lives at `crates/jellypilot-player/Cargo.lock`; both packages are excluded from the root workspace. Build output remains `target/iced-local-video`.
- iced fork `f812ae504989444eb57d9f8669c1ab5a2e326c95`; all GPU types come from iced re-exports. Shared `jellypilot-ui` fonts, dark theme, controls and Catalog surfaces are reused without modifying that crate.
- GStreamer core/app/video bindings pinned to 0.25.2, native floor 1.28. `gstreamer-base` is additionally constrained to 0.25.2: actual resolution selected 0.25.3, which failed compilation because it references `gst::pad_panic_to_error!`, absent from pinned core 0.25.2.
- `iced_video_player` 0.6.0 was not used: its iced 0.14/wgpu 27 types are incompatible with this fork's iced 0.15.0-dev/wgpu 29. The small appsink → CPU → texture architecture is retained, not its NV12 shader or playback abstraction.
- One OS worker owns playbin3, bus and appsink. Control messages are serialized; one latest-frame slot plus a two-buffer leaky appsink queue bounds frame retention. UI notifications coalesce independently from retained state/errors.
- No network playback, file picker dependency, subtitles, visualization, playlist, persistence, external-player fallback, webview, native child video window, HDR guarantee, or zero-copy/hardware-decoding guarantee.

## Layout and reuse boundary

- `crates/jellypilot-player/src/lib.rs`: public `Player`, `Status`, `AudioOutput`, `PlaybackPhase` and `PlaybackError` interface, with its usage contract in rustdoc.
- `crates/jellypilot-player/src/{playback.rs,video.rs,video.wgsl}`: private worker/pipeline, frame handoff and renderer implementation; backend and rendering regressions live beside that implementation.
- `experiments/iced-local-video/src/main.rs`: demo boot/update/view, CLI, path input, transport controls, drag preview, window shutdown and smoke orchestration. The demo uses a path dependency on the library and belongs to the isolated workspace via `workspace = "../../crates/jellypilot-player"`.

A caller creates `Player::new(AudioOutput::System)` and consumes the returned wake stream with `iced::Task::run`, calling `refresh` on each notification. Read `status()` for observed phase, playing intent, position/duration, seekability, volume/mute, pending open/seek and errors; use `ready()` / `can_seek()` to gate controls. Submit open, play/pause, seek, volume and mute through `Player` methods. Immediate command/refresh failures are `Result` errors; asynchronous open/seek failures appear in `Status::error`.

Compose `Player::view()` inside the caller's own layout, Canvas and transport overlay. Request/seek generations, sample slots, GStreamer/GPU types and worker ownership remain private; callers do not allocate tokens or manage renderer resources. The library does not impose window chrome, transport controls, persistence or a product playback session.

On shutdown, call `Player::close()` and **execute its returned iced task before closing the window**: it retires resources and requests stop immediately, then joins off the UI thread. Repeated close is safe; dropping a player requests shutdown without blocking but does not replace graceful completion. `AudioOutput::Discard` and the current-visible `uploaded_frame()` sequence diagnostic support unattended smoke, not audio acceptance or playback control.

## Native prerequisites and commands

Rust 1.98+, Bun, and GStreamer **1.28+** development/runtime packages are required for this isolated library/demo workspace, including its checks and tests; the library is not a native-dependency-free facade. Bindings do not install native libraries. On Arch Linux the relevant packages are `gstreamer`, `gst-plugins-base`, `gst-plugins-good`, `gst-plugins-bad`, `gst-plugins-ugly`, and `gst-libav`; package installation is a human/system-administration action. Other distributions need equivalent development packages and plugin packages. This promotion does not add GStreamer to the production workspace dependency graph.

Required playback elements: `playbin3`, `videoconvert`, `appsink`, `autoaudiosink`, an MP4 demuxer and H.264/AAC decoders. Self-contained lifecycle tests additionally require `videotestsrc`, `vp8enc`, `webmmux`, `vp8dec`, and `fakesink`.

Run from the repository root:

```sh
bun run task iced local-video run
bun run task iced local-video run --file test-videos/bbb_h264_aac_1080p.mp4
bun run task iced local-video check
bun run task iced local-video test
bun run task iced local-video clippy
bun run task iced local-video fmt --check
xvfb-run -a bun run task iced local-video run --smoke --file test-videos/bbb_h264_1080p_5mb.mp4
```

`run --release` is optional. `--file` is one argv element: quote paths containing spaces, Chinese characters or `#`. A missing action or unsupported option is rejected by the dispatcher. `test` accepts one optional test-name filter, not arbitrary cargo options. Never run cargo directly in this repository.

Every `iced local-video` action uses `--manifest-path crates/jellypilot-player/Cargo.toml`. `run` selects only `jellypilot-local-video`; `check`, `test`, `clippy` and `fmt` explicitly select **both** `jellypilot-player` and `jellypilot-local-video`, so moving implementation/tests into the library cannot silently remove them from the gate. Check/clippy retain `--all-targets`, clippy denies warnings, and the optional test filter applies to both packages. Non-format actions retain `--target-dir target/iced-local-video`; fmt has no target directory. Formal `iced run` / `iced hot` and root Rust command scope are unchanged.

Normal run uses real `autoaudiosink`; unattended smoke uses synchronized `fakesink` and **does not validate audible output**. Smoke must wait for a decoded frame's shader upload, then stop/join/close; startup alone is not success. Smoke errors and ten-second upload timeout must exit nonzero.

## Real acceptance media

Prepared on Linux x86_64 with native GStreamer core/app/video **1.28.6**. Existing user media was not overwritten, renamed or committed.

Source: [Blender official BBB download](https://download.blender.org/demo/movies/BBB/bbb_sunflower_1080p_30fps_normal.mp4.zip), 275,524,128 bytes. ZIP integrity was checked before extracting only its MP4. [License: CC BY 3.0](https://peach.blender.org/about/).

Attribution: **Blender Foundation 2008 / www.bigbuckbunny.org; Sunflower version: Janus Bager Kristensen 2013**. The full film and credits remain intact; video was copied, first audio track converted to stereo AAC. ffmpeg is preparation tooling, not a player dependency or sidecar.

```sh
ffmpeg -n -i test-videos/bbb-sunflower/bbb_sunflower_1080p_30fps_normal.mp4 -map 0:v:0 -map 0:a:0 -c:v copy -c:a aac -ac 2 -b:a 192k -movflags +faststart test-videos/bbb_h264_aac_1080p.mp4
```

Final `test-videos/bbb_h264_aac_1080p.mp4`: **634.600 s**, H.264 High, **1920×1080**, yuv420p, 30 fps; AAC-LC, 48 kHz, 2 channels; 253,882,349 bytes. Metadata was checked with ffprobe, not inferred from historical source metadata.

Local SHA-256 values (the publisher did not supply a ZIP SHA-256; these are not publisher-authenticated hashes):

| File | SHA-256 |
|---|---|
| Source ZIP | `e320fef389ec749117d0c1583945039266a40f25483881c2ff0d33207e62b362` |
| Extracted source MP4 | `ae51005850b0ff757fe60c3dd7a12d754d3cd2397d87d939b55235e457f97658` |
| H.264/AAC acceptance MP4 | `120794371208679049a29bd554ffb675fbf837cc1746ad10b6bc098a670a4c52` |

## Pre-promotion human visual/audio acceptance — 已通过

On 2026-09-08, before promotion into the reusable library, the user confirmed that manual acceptance passed (“人工确认通过”). This records the user's acceptance of the checklist below, not an agent-observed visual/audio result or a new acceptance run of the promoted library/demo. Agents did not drive the desktop or capture screenshots; headless smoke remains separate evidence.

- One window: video and iced controls compose together; the opaque transport card remains above video and clickable after resizing.
- Play the real 634.6-second film continuously for at least ten minutes without looping or skipping. Check dialogue/action synchronization, responsiveness, then seek repeatedly 0:30 ↔ 5:00 ↔ 9:00 and check synchronization again.
- Pause, drag to another scene: dragging previews time only; releasing updates the image without sound. Resume preserves correct audio/video timing.
- Resize wide/narrow repeatedly; the 1920×818 Sintel sample retains its display aspect ratio with centered borders, without restarting or seeking.
- Volume 100 → 20 → 0 changes audible output; mute/unmute preserves the chosen volume. Video-only inputs do not produce an audio error.
- Ended → replay starts from the beginning. Invalid media reports a recoverable error. Repeated replacement has no duplicate audio or stale image. Closing during playback stops audio and exits without hanging.
- For every original sample, record visible video/error, seeking, dimensions/aspect and decoder/limitations. H.264 is required; HEVC/VP9/AV1 are exploratory, with no hidden fallback.

For a quantitative resource record, run the real-audio player from your own terminal and collect actual continuous data with the existing sampler:

```sh
bun run task monitor --pid <experiment-pid> --out target/resources/local-video-10min.ndjson --samples 601 --interval-ms 1000 --label local-video
```

Human playback/synchronization acceptance has passed. No quantitative ten-minute RSS/CPU trace has been supplied with that confirmation. Do not substitute test runtime or a looped short clip for resource measurements. Compare steady playback and replacement RSS/CPU, together with appsink queue depth/drop counts and the actual renderer. CPU RGBA conversion/upload can be expensive; no unmeasured performance claim is made.

## Pre-promotion automated results (2026-09-08)

The results and implementation review details in this section describe the original standalone experiment before the source move and public API extraction. They are retained as historical evidence, not as proof that the promoted workspace has passed its new checks.

- `iced local-video check`, `test`, `clippy`, and `fmt --check`: passed. **11 tests** exercise real GStreamer lifecycle, paused seek/preroll timing, recovery/EOS/replay/shutdown, command-sender disconnect, FIFO rejection, replay followed immediately by seek, bounded frame retention, offset/padded/negative-stride RGBA, PAR containment, and scissor intersection.
- Dispatcher parser/command tests: **12 passed**, including Unicode/space/`#` argv preservation and rejection of malformed actions/options. Script typecheck, lint and formatting checks passed.
- Native `xvfb-run` smoke: all nine original video-only clips and the real H.264/AAC film decoded and uploaded a frame, stopped the worker and exited **0**. The nonexistent-path smoke reported `Open …: No such file or directory` and exited **1**. No experiment process remained after the batch.
- `bun run task rust check iced`: passed. Root `Cargo.lock` contains no GStreamer and retains SHA-256 `2cb12ba59ed0326d78d98cc3716d0a1415084e1f86255590d76eeb2df5d7f331`. Formal application manifests and run/hot entry semantics are unchanged.
- Independent source reviews covered lifecycle/concurrency and renderer/UI composition. Two lifecycle findings were corrected: replay now uses UI-allocated seek sequence numbers; non-regular paths are rejected before a blocking read. No outstanding findings were reported on the targeted recheck. Source review is not human visual acceptance.

Actual smoke observations below used the forced **wgpu** backend. The selected decode elements were logged from the live pipeline; they are observations on this host, not portable decoder requirements. The default GStreamer choice happened to use VA video decoders, but the transfer remains CPU-readable RGBA, not zero-copy.

| Input in `test-videos/` | Negotiated size / PAR | Selected decoder | Upload / exit |
|---|---|---|---|
| `bbb_h264_1080p_5mb.mp4` | 1920×1080 / 1:1 | `vah264dec` | Passed / 0 |
| `bbb_h264_720p_2mb.mp4` | 1280×720 / 1:1 | `vah264dec` | Passed / 0 |
| `bbb_h264_1080p_5mb.mkv` | 1920×1080 / 1:1 | `vah264dec` | Passed / 0 |
| `bbb_hevc_720p_2mb.mp4` | 1280×720 / 1:1 | `vah265dec` | Passed / 0 |
| `bbb_hevc_1080p_5mb.mp4` | 1920×1080 / 1:1 | `vah265dec` | Passed / 0 |
| `jellyfish_hevc_1080p_10mb.mp4` | 1920×1080 / 1:1 | `vah265dec` | Passed / 0 |
| `bbb_vp9_1080p_5mb.webm` | 1920×1080 / 1:1 | `vavp9dec` | Passed / 0 |
| `bbb_av1_1080p_5mb.mp4` | 1920×1080 / 1:1 | `vaav1dec` | Passed / 0 |
| `sintel_av1_1080p_5mb.mp4` | 1920×818 / 1:1 | `vaav1dec` | Passed / 0 |
| `bbb_h264_aac_1080p.mp4` | 1920×1080 / 1:1 | `vah264dec`, `avdec_aac` | Passed / 0 |

Each row establishes first-frame decoding/upload only. **Human visual/audio acceptance was separately confirmed by the user on 2026-09-08**; no additional per-file measurements were supplied. Paused seek is also proven with a self-contained WebM regression and the temporary real H.264 worker probe. Smoke snapshots reported queue depth 0–1 and zero appsink drops; these short observations are not sustained-performance measurements. Batch stdout/stderr is retained in ignored `target/iced-local-video/media-smokes.json` (the initial H.264 MP4 smoke was run separately).

The paused-seek regression originally timed out despite a new preroll arriving: calling `try_pull_sample` first clears appsink's preroll reference even on timeout. Paused polling now reads only preroll; the real H.264 probe returned PTS 5.033333333 s for a 5 s seek and remained paused. This probe was replaced by the final window entrypoint; no probe mode remains in the application.

## Promotion validation

Promotion changes the package boundary, library interface and command selection, not the intended window behavior or production MPV path. Verified on Linux on 2026-09-08:

- Isolated library+demo `check`, `test`, `clippy` and `fmt --check` passed: **13 regression tests and one compiled rustdoc example**. Public-interface regressions additionally cover replay/seek/pause without intervening refresh, invalid replacement preservation, decode-error recovery, and close-task completion with rejected post-close controls.
- `bun run check` passed, including **17 script tests**, script formatting/lint/typecheck, and production workspace Rust formatting/clippy. `bun run task rust test` passed: **955 tests passed, one ignored**.
- The real H.264/AAC film's headless smoke negotiated 1920×1080 RGBA, selected `vah264dec` / `avdec_aac`, uploaded frame 4 through wgpu with appsink queue/drop counts 0/0, joined and exited **0**. The missing-path smoke reported the actual open error and exited **1**.
- Root `Cargo.lock` retains SHA-256 `2cb12ba59ed0326d78d98cc3716d0a1415084e1f86255590d76eeb2df5d7f331`. GStreamer remains confined to the isolated player/demo dependency graph.
- Independent read-only reviews covered lifecycle/frame ownership and consumer/workspace/tooling boundaries; neither reported an actionable finding.

The nine-format matrix above remains pre-promotion evidence; this promotion reran the real H.264/AAC smoke, not that entire matrix. Prior human acceptance remains passed. No new visual/audio acceptance or sustained resource measurement is claimed. For a human post-promotion spot-check, run the same film and check the overlay after resizing, paused seek, mute/volume, replacement, replay and audible shutdown using the checklist above.

## Using the window

Without `--file`, the short H.264 path is prefilled but does not auto-open. Edit the path and press Enter or **打开** to open/replace. The transport card overlays the image; the bottom slider previews time while dragging and commits on release. Unknown duration is `--:--` and has no interactive seek control. Opening invalid paths preserves the current video; valid but undecodable media stops the old pipeline and reports a recoverable error. Volume/mute persist only during this experiment run. Window close disables interaction and joins the stopping worker on a blocking task before closing the native window.

Native tests on Unix also use the standard `mkfifo` utility for the non-regular-file regression. A file replaced concurrently between filesystem validation and GStreamer opening is not treated as a hostile-input security boundary. This is a disposable local experiment, not a sandbox for untrusted media.
