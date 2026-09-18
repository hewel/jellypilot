# Linux embedded HDR10 presentation

_Status: Accepted; HDR output is experimental. Manual presentation is human-accepted, content-driven Auto is implemented. Extends ADR 0040's embedded presentation policy. ADR 0041's Windows SDR-only policy is unchanged._

The existing embedded chain already uses 10-bit RGB10A2 video textures and a
10-bit Linux Vulkan presentation surface. Bit depth alone does not provide HDR:
the mpv host previously fixed the target at BT.709 / gamma 2.2 / 203-nit SDR,
so HDR sources were tone-mapped even on HDR-capable presentation chains.

The accepted setting is `hdr_output`: Auto / On / Off, default Auto, for
Embedded MPV on Linux Wayland only. Auto selects HDR for HDR content when
the presentation chain supports HDR10. On requests a whole-window HDR surface
whenever supported, including for SDR content mapped into PQ. Off stays SDR.
A Wayland surface has one color space; HDR cannot apply only to the video region.

Delivery was staged: phase 1 supplied manual On, Off, output status and the
fork/compositor changes. The user has confirmed that both HDR video and the UI
display normally on the tested device, accepting that stage. Phase 2 implements
content-driven Auto; its automatic transitions still require human display
acceptance. This does not establish a compositor/driver compatibility matrix.
Static HDR10 metadata (MaxCLL/mastering display) remains outside this change.
The feature remains explicitly experimental in Settings and user documentation
until broader display-chain and automatic-transition validation is available.
The accepted Auto / On / Off behavior and default Auto are unchanged; Off is the
recovery option for incorrect colors or brightness.

Auto observes the pinned decoder's `video-params/gamma`: `pq` and `hlg` request
HDR10 output; SDR, unknown and unavailable transfers do not. Source properties
avoid feedback from the renderer's HDR target. Bit depth, filenames, server
metadata and Dolby Vision profile labels alone are not HDR detection. MPV
continues to own source conversion into the chosen output target.

An event-driven, connection-local JSON IPC observer is independent of player
controls visibility, pause, and account sessions. It neither consumes the
playback controller's event queue nor captures duplicate player logs. Video
unload, an unavailable transfer, or disconnect clears the source state; pause
retains it. Loss in either MPV's event queue or the observer's delivery queue
invalidates the connection. Reconnecting reads a fresh initial source value
instead of retaining an unobservable stale mode. No additional source polling
or libmpv FFI event model is introduced.

## Presentation contract

The mpv fork's host ABI version 2 adds an optional target-color callback. Color
processing stays in gpu-next/libplacebo; producer image format stays
`A2B10G10R10_UNORM_PACK32`. HDR output targets BT.2020 / PQ with a fixed 203-nit
reference white and 1000-nit target peak. The existing GPU copy into a private
sampled texture remains; this is not a full-chain zero-copy claim.

The iced fork ports [iced-rs/iced#3420](https://github.com/iced-rs/iced/pull/3420)
to wgpu 30, with the corresponding
[cryoglyph update](https://github.com/iced-rs/cryoglyph/pull/4). Exact fork revisions
are pinned under `tools/embedded-mpv/`. The compositor requires Linux Wayland,
negotiated host ABI v2 and `BT2100_PQ` capability for its selected surface format
before requesting `SurfaceColorSpace::Bt2100Pq`. Capability enumeration proves
that the presentation chain accepts PQ signaling, not that the physical display
currently runs in HDR or reaches a particular luminance. No compositor/driver
version matrix is claimed validated without human evidence.

Linux embedded rendering uses a non-sRGB `Rgba16Float` scene attachment. HDR video
is decoded from PQ / BT.2020 into extended sRGB relative to 203 nits inside the
normal video primitive. Signed conversion preserves negative BT.709 components;
float storage retains values above one. Video remains in iced's ordinary drawing
order, so clipping, overlays, backdrop blur and screenshot capture include it.
The final pass converts the complete scene back to BT.2020 / PQ. Opaque UI white
therefore maps to 203 nits. In SDR mode that pass preserves scene code values.
Windows and External MPV keep their existing rendering paths.

This deliberately retains iced's existing `web-colors` gamma-domain blending
and blur, rather than introducing a separate linear-light UI/video blend. A
video-under-UI final pass was rejected because it bypassed scene effects and
capture. RGB10A2 is unsuitable for the scene attachment because its two alpha
bits quantize translucent coverage. The float scene costs one full-window GPU
presentation pass even in Linux embedded SDR mode; it avoids replacing renderer
pipelines on every HDR transition. Existing RGBA8 screenshots remain SDR previews
with clipped out-of-range highlights/gamut, not HDR captures or color-acceptance
evidence.

## Transitions and fallback

Changing target color advances a host frame epoch. Ready frames from the previous
epoch are discarded; a render spanning a target change is rejected when mpv
releases it. The cached video is invalid until a matching frame has been submitted
to the ordered copy queue, and the video region draws black meanwhile. A paused
player requests a redraw after a target change. Surface recreation compares
against the retained host target, not a new surface's initial state. Brief
reconfiguration flicker is accepted.

Unavailable HDR requests stay SDR without blocking playback. Settings receives
output-status changes through an event-driven watch stream; diagnostics records
transitions and fallback. Status describes the application's presentation encoding,
not physical monitor capability. HDR content on an SDR target retains mpv's
existing tone-mapping path. Human acceptance must check actual display mode,
highlights, neutral colors, translucent controls, backdrop blur, paused mode
switches, and last-window close/reopen; code-level smoke alone cannot establish
HDR appearance.
