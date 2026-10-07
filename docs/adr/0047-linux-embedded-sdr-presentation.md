# Linux embedded SDR presentation fallback

_Status: Accepted. Supersedes the Linux 10-bit presentation requirement in ADRs 0040
and 0041; extends ADR 0044 with an explicit 10-bit requirement for HDR10 output._

[Issue #232](https://github.com/hewel/jellypilot/issues/232) reports an X11 Vulkan
surface exposing only `Bgra8UnormSrgb` and `Bgra8Unorm`. Requiring a 10-bit window
surface prevents startup even when the GPU supports the internal 10-bit video
textures. Linux Embedded MPV Playback now uses the same SDR format preference as
Windows: `Rgb10a2Unorm`, then `Bgra8Unorm`, then `Rgba8Unorm`. sRGB attachments remain
excluded to preserve mpv's gamma-encoded SDR values without a second encoding.

MPV producer images, the synchronized copy and the private sampled texture remain
RGB10A2. Linux retains its `Rgba16Float` scene attachment; only final presentation
falls back to 8-bit, which may increase visible banding. The selected format remains
fixed for the retained device/engine and must be supported by reopened windows.
Startup diagnostics already warn when 8-bit presentation is selected.

HDR10 output still requires Linux Wayland, the negotiated host ABI, a selected
`Rgb10a2Unorm` surface and PQ signaling support. An 8-bit surface stays SDR, including
for HDR content or an explicit HDR On preference; mpv retains its SDR tone mapping
and Settings reports the requested HDR output as unavailable. Missing compatible
UNORM formats, internal texture support or pinned assets remain explicit errors.
There is no automatic External MPV fallback.

Format-selection tests establish the SDR fallback contract. Native smoke establishes
startup only; playback, colors, banding and HDR transitions require separate human
acceptance on the affected X11/Wayland display chains.
