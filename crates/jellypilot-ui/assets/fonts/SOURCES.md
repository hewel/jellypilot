# Bundled font sources

JellyPilot embeds the following unmodified local font files. Both are available in either UI language: Manrope V5 is the primary body and heading family; MiSans VF supplements Chinese text, including mixed-script names in the English interface. Native tray/menu fonts remain platform-managed.

| Local file | Typeface version and original axes | Exact supplied source | SHA-256 |
| --- | --- | --- | --- |
| `ManropeV5VF.ttf` | 5.000; typographic family `Manrope V5`, legacy family `Manrope V5 ExtraLight`; `wght` 200–800, default 200; 159,428 bytes | `/home/hewel/Downloads/themes-fonts/manrope/variable/ManropeV5VF.ttf` | `89e6661b47ecc06cbc857a5804bf43a3ab7503a8bbc1b3cfed545e4ddc561f8c` |
| `MiSansVF.ttf` | 4.009; family `MiSans VF`; `wght` 150–700, default and named Regular 330; 20,093,424 bytes | `/home/hewel/Downloads/themes-fonts/MiSans/MiSans/可变字体/MiSansVF.ttf` | `0ddef90648998900175cfdca9a6f087a2544c182f130b0ad4f7e94a03a115e79` |

The Downloads paths record provenance only. Compiled resources use the copies in this directory. No subsetting, static font generation, metadata rewriting, renaming, or other font-file modification is performed.

## License and attribution

These are **not OFL font releases**. The obsolete Inter and Space Grotesk assets and OFL notices are replaced by these agreements:

- `MANROPE-LICENSE.txt`: the author's **Manrope V5 Font Software License Agreement, version 1.0 — April 5, 2025**, transcribed without the decorative icons from <https://www.sharanda.com/manrope> on 2026-09-07. The embedded `https://shimmer.cloud/manrope-v5-license.txt` address redirects to a missing page; the author's current page supplies the agreement. This permits personal/commercial use, embedding, and redistribution of the unmodified font under its original name. It prohibits modification, reverse engineering, derivatives, and standalone sale. Required attribution: **Manrope V5 by Mikhail Sharanda**. The notice includes the author's current copyright and retains the original font's earlier copyright verbatim.
- `MISANS-LICENSE.pdf`: exact official bilingual agreement downloaded on 2026-09-07 from <https://hyperos.mi.com/font-download/MiSans%E5%AD%97%E4%BD%93%E7%9F%A5%E8%AF%86%E4%BA%A7%E6%9D%83%E8%AE%B8%E5%8F%AF%E5%8D%8F%E8%AE%AE.pdf>.
- `MISANS-LICENSE.txt`: human-readable full bilingual extraction of that PDF, with the original font's copyright notice prepended: **Copyright © 2020-2025 Beijing Xiaomi Mobile Software Co.,Ltd. All Rights Reserved.** Section 2 requires an in-software indication that MiSans is used and retention of copyright and the agreement; it prohibits adaptation/redevelopment and restricts standalone font distribution. MiSans is an application-embedded resource, not a separately distributed font product. The official embedding FAQ is <https://hyperos.mi.com/font/en/faq/>.

`fonts::FONT_ATTRIBUTIONS` exposes the in-software notices; `fonts::FONT_LICENSES` embeds both full agreements for the expandable Settings legal text. These legal/source texts are retained in their original language rather than treated as UI translations.

## Renderer matching

Call `jellypilot_ui::fonts::initialize()` before constructing the iced application. It loads both bundled fonts into iced's shared font system, preserves iced's icon and other system fonts, and configures MiSans before platform Han/common fallbacks. Other scripts retain the platform fallback lists. This is independent of UI language; Manrope remains the primary family even for Chinese UI text.

Body text requests weight **400** and headings **600**. In iced 0.14 / cosmic-text 0.15 / fontdb 0.23, fontdb records OS/2 weight rather than a variable font's full weight range, and cosmic-text's primary/configured-fallback iterator requires an exact-weight matching descriptor. Initialization therefore registers in-memory face descriptors for the two semantic weights, sharing the original font source. The original 200/330 descriptors, family names, file bytes, and named instances remain unchanged. Cosmic-text's shaper and Swash rasterizer apply the actual `wght` variation at the requested weight; this is not a claim that MiSans's named Regular is 400. Installed copies with the same family names are removed from the runtime database in favor of the supplied bundled source.

Primary implementation evidence (versions installed by this project):

- iced graphics' public shared font system, raw access, registration, and font-to-weight conversion: <https://docs.rs/iced_graphics/0.14.0/src/iced_graphics/text.rs.html>.
- cosmic-text's configurable fallback and primary → script → common ordering, including its exact-weight filter: <https://docs.rs/cosmic-text/0.15.0/src/cosmic_text/font/fallback/mod.rs.html>.
- fontdb's public runtime `push_face_info` seam and typographic-family parsing: <https://docs.rs/fontdb/0.23.0/src/fontdb/lib.rs.html>.
- cosmic-text's `wght` shaping and rasterization: <https://docs.rs/cosmic-text/0.15.0/src/cosmic_text/font/mod.rs.html> and <https://docs.rs/cosmic-text/0.15.0/src/cosmic_text/swash.rs.html>.

The display-free behavior check in `fonts.rs` exercises actual mixed-script shaping, selected family/weight, and raster differences from each font's default axis weight. Runtime verification remains the integrator's responsibility. A passing code-level check does not establish visual appearance, universal glyph coverage, or Windows/macOS/Linux acceptance. The supplied files total 20,252,852 bytes (19.31 MiB); executable/installer size, startup cost, and memory impact are not measured here.
