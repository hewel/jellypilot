# Keep UI Language in presentation with isolated language-setting commits

_Status: Accepted in the 2026-09-06 design interview; implemented with code-level validation. Cross-platform visual acceptance remains human. Amends ADR 0027's bundled typography choice without changing the iced frontend or External MPV Playback architecture._

JellyPilot uses embedded Fluent resources through `fluent-templates` for its own interface text in both App Modes. The initial languages are English and Simplified Chinese: English and the core Chinese flows must be complete, while other missing Chinese messages may fall back to English. Translations ship with the binary rather than as external language packs. The [UI internationalization specification](../i18n-design-spec.md) defines the full product, fallback, typography, and acceptance contract.

## Presentation Ownership

Keep the saved UI Language Preference and pure supported-language resolution in the existing core settings/model layer, while the iced application owns the Fluent loader, effective language state, and message formatting. Share immutable resources, not a mutable global current language. Retain semantic operation/error data until rendering so manual switching can reformat existing feedback and cached labels without resetting login, dialogs, or playback. Logs and support exports remain untranslated; media-server content, audio/subtitle preferences, and MPV's own interface are independent of UI Language.

This avoids pushing translated strings and Fluent dependencies into business crates, where they would freeze the language of already-produced results. Follow system remains a persisted preference resolved at startup or explicit reselection, with script-aware matching before invoking the loader; generic loader negotiation does not define the product's Traditional-versus-Simplified policy.

## Language-Only Live Commit

A language selection preserves unrelated edits already on disk but adopts only the language field into the running settings snapshot. Other settings operations keep their existing semantics for pre-existing fields; they save the latest disk language unchanged while retaining the currently committed live preference. Startup and explicit language selection are the live-language commit boundaries. This uses the existing store and persistence machinery, not another configuration file or a redesign of unrelated settings behavior.

The pre-localization generic settings mutation reloaded and adopted the entire disk snapshot. Reusing that operation unchanged could make a language selection apply a pending App Mode or subtitle change, close the Settings Modal, or reconfigure playback. The user rejected that coupling. Commit the live language only after persistence succeeds, and determine locale refresh from live state and the requested System resolution rather than solely from whether file bytes changed.

## Bundled Typography

Use the supplied, unmodified Manrope V5 variable TTF for body and heading text it covers, with the supplied MiSans VF as the Chinese supplement in either UI Language. This replaces ADR 0027's Inter/Space Grotesk selection; it does not replace native tray-library typography or introduce a second styling system.

The choice accepts an approximately 18.35 MiB increase in raw font resources to avoid relying on installed Chinese fonts. Both supplied versions have their own licenses, not the existing assets' OFL terms: preserve their identities and license texts, satisfy the documented attributions and distribution restrictions, and do not subset or modify them. Source identity and actual renderer family/weight matching are verified, and the Linux native startup smoke passes. Installer size, startup performance, resident-memory impact, and cross-platform visual acceptance remain unmeasured or human-owned; see the specification's implementation verification and acceptance checklist.
