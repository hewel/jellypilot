# Windows embedded SDR presentation formats

_Status: Accepted. Extends ADR 0040 with Windows Embedded MPV support and its presentation policy. Linux's default and 10-bit presentation requirement are unchanged._

Windows supports the packaged, pinned host-enabled libmpv through the shared Vulkan host.
External MPV remains the Windows default; Embedded is selected in Settings or with
`--embedded`. The Windows baseline requests `d3d11va-copy` independently of the surface format.

A Windows Intel Arc startup log reported a compatible Vulkan adapter but no native
`Rgb10a2Unorm` presentation surface. Requiring that format prevented embedded startup
before loading libmpv, even though the adapter could potentially render 10-bit textures.

The host selects `Rgb10a2Unorm` when advertised; Windows may otherwise select
`Bgra8Unorm`, then `Rgba8Unorm`. The selected format is retained with the device context
and used by iced's engine, window configuration and presentation. Reopened windows must
support that same format because they share the retained engine. Startup logs identify
the selected format and warn when presentation uses 8-bit output.

MPV's producer images, private video texture and synchronized copy remain RGB10A2.
Final UI composition (including iced's intermediate targets) and window presentation
use reduced precision on the 8-bit path, which may increase visible banding. Neither
path claims HDR presentation. sRGB attachments are
excluded: the video shader passes through mpv's gamma-encoded SDR values and iced uses
`web-colors`, so automatic sRGB encoding would change both video and UI colors.

Missing compatible formats remain explicit errors; there is no automatic External MPV
fallback. Format negotiation alone does not establish playback, hardware decoding or
visual color acceptance; those require validation on the affected Windows desktop.
