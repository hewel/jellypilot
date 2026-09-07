# JellyPilot UI Internationalization Specification

**Status: Implemented; code-level validation complete.** The design confirmed on 2026-09-06 was implemented following the user's subsequent request for the complete implementation. Human visual and cross-platform acceptance remain separate, as recorded below. No commit or publication is included.

## References and Existing Boundaries

- Domain vocabulary: [CONTEXT.md](../CONTEXT.md).
- Native frontend and External MPV Playback: [ADR 0027](adr/0027-cross-platform-iced-frontend.md).
- Surface ownership and shared application state: [ADR 0029](adr/0029-iced-surface-modules.md).
- Accepted localization and language-save decisions: [ADR 0035](adr/0035-ui-language-presentation-and-isolated-settings.md).
- Verification scope: [validation policy](agents/validation.md).
- Supplied reference: `/home/hewel/Downloads/Iced_Fluent_Templates_Integration_Guide.md`, reviewed September 6, 2026, targeting iced 0.14.0 and fluent-templates 0.15.1. Its example was not compiled or run by its author; it is guidance, not integration evidence.

The existing settings implementation persists `config.json` through `SettingsStore` in `crates/jellypilot-core/src/config.rs`. UI Language must use the existing settings infrastructure rather than introducing another configuration file. ADR 0027's historical TOML description does not describe the current serializer.

## Confirmed Delivery Contract

| Decision | Contract |
| --- | --- |
| Coverage | Whole application, not a representative-screen-only delivery |
| Initial languages | English (`en-US`) and Simplified Chinese (`zh-CN`) |
| Preference | Follow system or select a fixed language; persist the preference |
| Manual switching | Immediate, without restarting JellyPilot or interrupting playback or login |
| Resource lifecycle | Translations embedded in the application and released with its binary |
| Default | Follow system, for both fresh installs and existing configurations without a UI Language Preference |
| No supported system preference | Use English |
| System preference refresh | At application startup and whenever the user explicitly selects Follow system, including reselecting it |
| Running-system changes | No continuous subscription or focus-triggered detection is promised |
| Traditional Chinese | Do not silently substitute Simplified Chinese; continue through the remaining system preferences |
| Logs | Do not translate diagnostic event bodies, technical log details, or support exports |
| Formatting | Localize reading-oriented count/duration messages; retain invariant technical formats |
| Selector access | Login surface and Settings → Interface, sharing one preference |
| Typography | Manrope V5 for English/other covered non-CJK text, including headings; MiSans VF supplements Chinese |
| Translation completeness | English complete; core Chinese flows complete; other Chinese messages may fall back to English |

Coverage includes both Full and Control-Only App Modes: login/account flows, Library Browser surfaces where present, Now Playing, the Settings Modal, dialogs, placeholders, tooltips, in-app notifications, tray menus, and JellyPilot-generated user-facing errors. Diagnostics controls are included; log contents are explicitly excluded.

UI Language does not translate server-provided media names, descriptions, or other content. It does not change media metadata requests, audio/subtitle preferences, or the external MPV application's own interface. Existing product and provider names are not new language variants.

A manual language change must reach already-visible app-owned surfaces, not only views opened afterward. Existing dialogs and feedback retain their semantic meaning; switching is not permission to clear them or reset the active business flow.

Embedded resources exclude external language-pack installation and independent translation updates. Development-only translation hot reload has not been requested.

## Confirmed System-Language Policy

Inspect system preferences in their reported order and select the first supported match. Matching language candidates and falling back for an individual missing message are separate operations.

| Candidate or ordered preference list | Result |
| --- | --- |
| `en-US` or another English region such as `en-GB` | `en-US` |
| `zh-CN`, `zh-SG`, `zh-Hans`, or unqualified `zh` | `zh-CN` |
| `zh-TW`, `zh-HK`, or explicit `zh-Hant` | Not a Simplified Chinese match; try the next preference |
| `zh-Hant-CN` | Preserve the explicit Traditional script; do not infer Simplified from the region |
| `zh-TW`, then `en-US` | `en-US` |
| `fr-FR`, then `zh-CN` | `zh-CN` |
| No supported candidate | `en-US` |

A user can always select Simplified Chinese explicitly, including on a Traditional Chinese system. Persist Follow system as a preference, not as the language that happened to resolve during the last run.

## Logs and User Feedback

Do not translate log event bodies, raw technical details, or support exports. Existing sanitization remains mandatory; choosing not to translate is not permission to expose unsanitized third-party error text.

Translate Diagnostics titles, filters, actions, and other interface labels, but do not introduce a localized-event-summary model for this task. Ordinary login, playback, settings, and other user-facing failures still need translated explanations. A user-visible failure and a technical log entry are different presentation contracts even if they originate from the same operation.

## Formatting and Partial Translations

Translate complete count messages and their plural grammar, and reading-oriented durations such as “1 hour 30 minutes.” Numeric arguments remain numeric where Fluent selects plural categories. Playback timecodes, IEC byte units, protocol values, and diagnostic UTC timestamps keep stable technical formats. No separate region-format preference or comprehensive date/number-localization expansion is part of this delivery.

Every app-owned message within the coverage boundary must use the localization path, and the English fallback must be complete. Chinese must cover the language selector, login, settings, principal playback controls, and their critical action feedback. Outside these core flows, Simplified Chinese may be incomplete, with missing messages falling back to English. Disclose this possibility in the language-setting help and report untranslated messages rather than presenting Chinese as fully translated.

An absent non-core Chinese message is permitted; missing core Chinese messages, malformed FTL, invalid supplied arguments, broken references, and an absent English fallback are defects. Resource checks must distinguish missing translations from formatting failures and must inspect exact-language resources rather than accidentally validating the fallback.

## Language Selector and Fonts

Expose a lightweight selector before authentication on the login surface and a selector in Settings → Interface. Both edit the same saved preference. Use language self-names, `English` and `简体中文`, so users can recover after selecting a language they cannot read.

Bundle a general Chinese-capable font alongside the application instead of requiring a system Chinese font or shipping only a fixed UI-glyph subset. This improves dynamic Chinese names as well as app-owned text, without promising universal Unicode coverage.

Use the user's complete, unmodified TTF files, not their web-font variants:

| Role | Local source | Inspected metadata | Raw size |
| --- | --- | --- | --- |
| Primary body and heading font | `/home/hewel/Downloads/themes-fonts/manrope/variable/ManropeV5VF.ttf` | Version 5.000; typographic family `Manrope V5`, legacy family `Manrope V5 ExtraLight`; `wght` 200–800, default 200 | 159,428 bytes / 155.69 KiB |
| Chinese supplement | `/home/hewel/Downloads/themes-fonts/MiSans/MiSans/可变字体/MiSansVF.ttf` | Version 4.009; family `MiSans VF`; `wght` 150–700, default and named Regular 330 | 20,093,424 bytes / 19.16 MiB |

This replaces both Inter and Space Grotesk in the intended iced typography; preserving their old body/heading split was overridden by the user's Manrope choice. Keep both new fonts available in both UI Languages: Chinese server names still need Chinese glyphs when the interface is English. Native tray/menu typography remains governed by its platform library, not the iced font selection.

FontTools successfully parsed and decompiled both fonts' tables. Manrope covers all printable ASCII. MiSans covers 20,976 code points in the basic CJK unified range and 6,582 in Extension A; the interview's Chinese UI/punctuation sample had no missing characters. These are file-level observations, not proof of iced rendering, universal character coverage, or correct cross-platform font matching.

The selected files total 20,252,852 bytes (19.31 MiB), versus 1,013,252 bytes for the previously bundled Inter/Space Grotesk pair: an 18.35 MiB raw-resource increase. Executable size, compressed installer size, startup cost, and resident-memory impact have not been measured. Weight selection must be verified explicitly: neither Manrope's default axis value 200 nor MiSans's named Regular value 330 justifies assuming conventional 400-weight rendering without checking the actual renderer.

File identities:

- MiSans VF SHA-256: `0ddef90648998900175cfdca9a6f087a2544c182f130b0ad4f7e94a03a115e79`.
- Manrope V5 VF SHA-256: `89e6661b47ecc06cbc857a5804bf43a3ab7503a8bbc1b3cfed545e4ddc561f8c`.

### Font Distribution Terms

These supplied versions are not covered by the existing fonts' OFL files:

- [MiSans's official FAQ](https://hyperos.mi.com/font/en/faq/) permits embedding. Its [license agreement](https://hyperos.mi.com/font-download/MiSans%E5%AD%97%E4%BD%93%E7%9F%A5%E8%AF%86%E4%BA%A7%E6%9D%83%E8%AE%B8%E5%8F%AF%E5%8D%8F%E8%AE%AE.pdf), section 2, requires in-software attribution and retention of copyright and the agreement, prohibits adaptation/redevelopment, and restricts separate distribution of the font itself. Treat it as an application-embedded resource, not a standalone font download or an OFL asset.
- The supplied Manrope file identifies the **Manrope V5 Font Software License Agreement**, not the older OFL release. Its embedded `shimmer.cloud/manrope-v5-license.txt` URL redirects to a missing page. The [author's current Manrope page](https://www.sharanda.com/manrope) publishes version 1.0 of the agreement, dated April 5, 2025: embedding and redistribution are allowed if the file remains unmodified and retains its original name; modification, reverse engineering, derivatives, and standalone sale are prohibited. Include the requested attribution, “Manrope V5 by Mikhail Sharanda”.

During implementation, preserve the exact source identities and correct license texts using the existing `assets/fonts/SOURCES.md` convention, and provide the required software attribution. Do not subset, rewrite font metadata, or relabel either font under OFL. Attribution/license text is legal/source content, not a requirement to translate logs or third-party documents.

## Accepted Integration Design

Use Fluent through `fluent-templates` as described by the supplied guide, with a small application presentation module rather than a second translation engine.

- **Settings and negotiation:** `jellypilot-core` owns the saved preference and display-free resolution policy through its existing settings infrastructure. The app reads system candidates; pure resolution accepts an ordered candidate list, so tests need not mutate the process or desktop locale. Preference is device-level like existing interface settings, not owned by a Saved Service Profile.
- **Formatting:** the iced application owns embedded FTL resources and their shared loader. Store the effective supported language in shared application state; do not introduce a mutable global current language or Fluent dependencies into business/domain crates.
- **Interface:** callers supply the current locale, a stable message identifier, and typed values for arguments. Format complete translation units; avoid concatenating translated sentence fragments. Small domain-specific format helpers are appropriate where they eliminate repeated argument construction, not as a handwritten translation DSL or an unproven compile-time-key-checking claim.
- **Error and view state:** keep operation/error kinds and required data until rendering, rather than storing already-translated strings. Preserve existing typed states where available. Move app-owned prose produced by domain helpers to presentation; preserve raw server names, identifiers, technical details, and log text.
- **Switching:** reuse the existing settings storage and app message routing, but give the language mutation the language-only live-commit semantics below; the current generic settings mutation is not sufficient unchanged. Explicitly refresh tray labels and cached app-owned text. Cached rows such as “Latest {library name}” need semantic row identity plus the unchanged server name. Locale participates in any real localized cache key; do not add resource-reload revisions or new caches for static embedded resources without a concrete need.
- **Cost:** initialize the shared loader once, not in `view`; avoid repeatedly parsing locales or reading the OS during rendering. Do not allocate argument maps for argument-free messages. No new watchers, external packs, polling loop, or broad translation cache is required.
- **Typography:** keep font selection and matching in `jellypilot-ui`, with semantic body/heading weights and Chinese fallback. Do not choose the entire text widget's font solely from UI Language, because strings can mix scripts. Reuse existing renderer shaping/fallback support rather than adding a per-character widget-splitting mechanism.

### Edge Semantics

- Missing preference in an existing configuration means Follow system. An invalid or unsupported saved language value recovers through that default without discarding unrelated settings or failing startup solely for the language field, following existing tolerant interface-enum deserialization.
- Parse language, script, and region before applying aliases; ignore valid locale extensions for UI matching rather than feeding a full locale string to a narrower parser. Skip malformed or unsupported system candidates and continue to the next one. `C`/`POSIX` alone resolves to the English fallback.
- Explicit script wins over region: Hans can match Simplified Chinese even with another region; Hant never becomes Simplified merely because its region is CN. Without an explicit script, the configured Simplified aliases are unqualified `zh`, CN, and SG; traditional-region candidates such as TW/HK/MO are not substituted.
- Pass only the resolved supported ID (`en-US` or `zh-CN`) to the loader. The loader's own negotiation is not the product's script policy.
- A successful manual selection preserves authentication/profile identity, in-flight login operations, playback position/queue, draft inputs, active dialogs, and unrelated settings. Text reformatting must not replay operations or restart toast lifetimes.
- Language selection commits only the language field to the running settings snapshot. When saving, preserve unrelated edits already present on disk, but do not adopt those edits into the live snapshot, trigger their App Mode/playback side effects, or overwrite them with stale in-memory values. Other settings operations retain their existing behavior for pre-existing fields; they preserve pending disk UI Language edits without importing that preference into the running snapshot. Startup and explicit language selection are the live-language commit boundaries. This is implemented within the existing store/persistence machinery, not with a second config file or a general settings redesign.
- Determine locale refresh from the previous live preference, the selected preference, and a fresh System resolution when requested—not solely from a “file was changed” boolean. If disk already contains the selected language but live state differs, selection must still update the UI.
- A failed preference save follows the existing save-before-commit contract: keep the previously active preference and locale, and show a localized persistence failure. Do not appear to save a language that will silently revert on restart.
- Reselecting Follow system re-resolves system candidates even if the persisted preference did not change.
- `JellyPilot`, `Jellyfin`, `Emby`, `MPV`, server-provided names, and the current brand-only native window title remain unchanged. No new translated window-title wording is required.
- Retain Fluent's normal bidirectional isolation for displayed interpolated text. Copy server addresses, paths, and diagnostic data from their original values, not from formatted UI sentences.

### Upstream Evidence and Limits

- [`static_loader!` source](https://docs.rs/crate/fluent-template-macros/0.15.1/source/src/lib.rs) embeds source bytes at compile time but lazily parses resources/builds bundles at runtime. Resource initialization must be exercised; a Rust type check alone does not establish valid FTL.
- [`StaticLoader` lookup source](https://docs.rs/fluent-templates/0.15.1/src/fluent_templates/loader/static_loader.rs.html) confirms normal lookup performs fallback. Exact-language validation uses `lookup_single_language`; `lookup_no_default_fallback` still allows other negotiated candidates.
- [Fluent language negotiation](https://docs.rs/crate/fluent-templates/0.15.1/source/src/languages.rs) treats available locales as ranges. An omitted script in the available locale can match an explicitly requested script, so raw `zh-Hant-CN` must not be delegated to a `zh-CN` bundle's negotiation.
- [`sys-locale` 0.3.2](https://docs.rs/sys-locale/0.3.2/sys_locale/) exposes ordered preferences, not language-change notifications. On Linux its values come from the running process environment; re-reading cannot promise detection of desktop changes that are not reflected there.
- [`fluent-bundle` 0.16 number formatting](https://docs.rs/fluent-bundle/0.16.0/src/fluent_bundle/types/number.rs.html) does not implement full region-aware numeric formatting; date/time is not a built-in replacement for a dedicated formatter. The chosen narrow formatting contract deliberately avoids promising full ICU behavior.
- [iced 0.14 shaping](https://docs.rs/iced_core/0.14.0/src/iced_core/text.rs.html) defaults to Auto when neither shaping override feature is enabled: ASCII uses Basic and non-ASCII uses Advanced, including fallback. Enabling `advanced-shaping` globally is not required merely to obtain CJK fallback. Required glyphs must still exist in an available font.

## Acceptance Contract

Implementation must demonstrate the following behaviors, not merely compile a loader:

1. New and pre-language configurations start with the supported system language, with no account/login prerequisite; unsupported candidates fall through in order and ultimately use English.
2. Fixed English, fixed Simplified Chinese, and Follow system persist distinctly across restarts. Invalid language fields do not discard otherwise valid settings.
3. Explicit Hant versus Hans, region aliases, a supported secondary preference, malformed inputs, and reselecting Follow system obey the policy above.
4. Login and Settings selectors edit one preference, display recognizable self-names, and retain current inputs and in-flight work across manual switching.
5. Switching from either language and back preserves active playback, queue, profile, dialogs, and feedback semantics. Existing toasts do not acquire a new lifetime merely by changing language. Include a case where disk has pending App Mode/subtitle changes: the language save preserves them on disk without applying them live, and an already-equal disk language still updates a different live preference.
6. Already-loaded home/list labels and available native tray controls update without requiring a content reload or playback-state change. Brand-only native titles stay unchanged.
7. Log bodies, technical details, export text/UTC time, media metadata, and audio/subtitle preferences do not change with UI Language.
8. Both resources initialize; English is complete; required Chinese core flows are complete. Non-core missing Chinese messages fall back correctly and are reported separately from invalid messages.
9. Parameterized messages handle zero, one, and two; counts use numeric values for selection. Preserve the chosen fixed timecode/technical formats and correct Chinese sentence structure.
10. Fonts load without depending on the original Downloads paths or installed copies. Both fonts are available in either language, with actual selected families and weights verified rather than inferred from filename or successful TTF parsing.
11. Human visual acceptance covers login, Settings, Now Playing, sidebar, list/detail views, and tray in both App Modes where present: mixed Chinese/English, long labels, input text, truncation/tooltips, weight hierarchy, and small-size legibility in both themes. Font fallback and layout need Windows/macOS/Linux acceptance; agent file checks do not substitute for it.

Follow the [validation policy](agents/validation.md): focused changed-contract tests and clippy, Rust formatting, then workspace gates for cross-crate implementation (`bun run check` and `bun run task rust test`). Add `xvfb-run -a bun run task iced run --smoke` for startup/settings/tray/font integration. The smoke gate proves startup, not appearance or cross-platform correctness. No Cargo invocation bypasses the Bun dispatcher.

The implementation and code-level acceptance are complete. Human visual and cross-platform acceptance remain separate; see the records below.

## Design Verification

Independent read-only reviews covered the product/integration contract and font/distribution claims. The settings disk-merge conflict was resolved through an explicit user decision for language-only live commits and passed bounded closure review. Local documentation links, source references, commands against repository policy, and font metadata were checked; these checks do not establish implementation or visual correctness.

## Implementation Verification

- [Core locale policy](../crates/jellypilot-core/src/locale.rs) and [settings persistence](../crates/jellypilot-core/src/config.rs) implement tolerant preference loading, ordered/script-aware negotiation, and isolated live-language commits. Regression coverage includes external disk edits, already-equal disk language, no-op generic saves, missing files, and unreadable/malformed saves.
- The [presentation localizer](../src-iced/src/i18n.rs) embeds 410 English and 410 Simplified Chinese message IDs. The delivered catalogs have no missing Chinese messages. Exact-language checks validate resources and argument contracts with numeric zero/one/two; invalid formatting is not accepted as a missing translation. A temporary removal of a non-core Chinese message exercised the actual English fallback and missing-translation report, then was restored.
- Headless application regressions cover immediate retranslation of retained feedback without adopting unrelated settings, explicit System reselection, visible save failures on Login and the Full Settings Modal, preservation of authentication feedback, and input focus across toast creation/dismissal. Playback diagnostics retain distinct technical warnings even when their translated summaries are identical.
- [Bundled font matching](../crates/jellypilot-ui/src/fonts.rs) is exercised through iced shaping with mixed-script text and actual 400/600 weight variations. Both installed TTFs match the supplied originals; provenance, licenses, and required attributions are recorded in [font sources](../crates/jellypilot-ui/assets/fonts/SOURCES.md) and exposed in Settings.

| Code-level gate | Result |
| --- | --- |
| `bun run task rust test core` | Passed: 154 unit tests and 27 browse adapter tests |
| `bun run task rust test iced` | Passed: 127 UI tests and 234 application tests |
| `bun run task rust clippy core` / `bun run task rust clippy iced` | Passed |
| `bun run task rust fmt --check` | Passed |
| `bun run check` | Passed: formatting, lint, script typecheck, 14 script tests, and workspace clippy |
| `bun run task rust test` | Passed for the maintained workspace |
| `xvfb-run -a bun run task iced run --smoke` | Passed on Linux: native startup, one rendered frame, normal exit |

Independent read-only locale/config, presentation, and typography/tray reviews completed, and confirmed findings received closure review. The native smoke emitted Vulkan DRM-extension and GLES-context warnings but exited successfully. It does not exercise the live tray (smoke mode disables it), establish appearance, or validate Windows/macOS. The existing platform-specific tray threading/event-loop design is unchanged. Installer size, startup performance, and resident-memory impact have not been measured.

## Human Acceptance Checklist

- On Windows, macOS, and Linux, check both light/dark themes and both App Modes where a surface exists. Include a clean environment without separately installed Manrope/MiSans; check mixed Chinese/English, punctuation, long labels, small text, body/heading weight hierarchy, truncation, and tooltips.
- On Login, switch English → 简体中文 → Follow system and back while retaining server/username/password inputs, authentication feedback, and an in-flight Quick Connect attempt. Restart to verify the three distinct preferences. A failed language save must leave the old selection active and show an explanation without replacing authentication feedback.
- In Settings → Interface, switch languages in both the Full modal and Control-Only window. Keep playback, queue, draft fields, and dialogs intact. Verify that visible feedback retranslates without a fresh timeout, and that toast appearance/dismissal does not reset input focus, caret position, or scroll position.
- In Full mode, check Sidebar, Home, browsing/search, Detail, and Personal Lists after data has loaded. App-owned headings/actions should change immediately; server names, media titles/descriptions, and other server content must remain unchanged.
- In the player bar, Control-Only Now Playing, and available native tray menus, change language without changing playback state. Verify immediate control-label updates, unchanged playback timecodes/technical units and audio/subtitle choices, and singular/plural Media Info counts such as mono versus stereo audio.
- Check localized Diagnostics controls while keeping event bodies, technical details, exported text, and UTC timestamps unchanged. Confirm original server addresses are copied without localized wrappers. Open font licenses in Settings and verify the attribution and full original agreements are readable; third-party license text is intentionally not translated.
