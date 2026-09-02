# 0018. Move Library Browse policy into a portable Rust core

## Status

Accepted

## Context

The Library Browse route needs the same paging policy across the current Solid UI and any future
native UI. Its policy includes query generations, page-zero gating, visible-page planning, bounded
prefetch, request deduplication, retry, stale-completion rejection, and page retention. Keeping that
policy inside a Solid/TanStack module makes it difficult to reuse and lets transport or rendering
lifecycle details become part of the policy contract.

The route also runs in two browser environments: a normal browser during focused development and
tests, and the Tauri WebView in the shipped desktop application. Both can execute WebAssembly, but
they do not necessarily use the same page transport. The desktop application loads pages through
typed Tauri commands; a browser-only application would need an HTTP/authentication adapter.

## Decision

Put the synchronous, metadata-only Library Browse state machine in the Tauri-free
`crates/jellypilot-core` crate. The core accepts browse inputs and emits ordered commands. It does
not fetch data, retain media item payloads, observe the DOM, render UI, or depend on Tauri, Solid,
TanStack, Effect, or a media-server SDK.

The input vocabulary is `Configure`, `WindowChanged`, `LoadNext`, `Retry`, and token-correlated
`PageSettled`. The output vocabulary is `ResetViewport`, `LoadPage`, `CancelLoad`, and
`ReleasePages`. `LoadPage` carries start/limit, priority, cache mode, and an opaque request token;
adapters must return that token when settling the request.

Expose that state machine to browser-based UIs through `crates/jellypilot-core-wasm`. The wrapper is
limited to `wasm-bindgen`/TypeScript DTO conversion. The Tauri WebView and a normal browser load the
same browser-targeted WASM module; JellyPilot does not call browse policy over Tauri IPC. A future
Rust-native UI may link `jellypilot-core` directly and skip the WASM wrapper.

Keep environment-specific page loading outside the core:

- the current desktop adapter executes `LoadPage` through the generated, typed Tauri command;
- a browser-only product can implement the same effect with an HTTP/authentication adapter;
- tests can use an in-memory adapter and settle requests deterministically.

The Solid facade keeps its existing consumer-facing API. `src/utils/libraryBrowseWasm.ts` lazily
imports and initializes the WASM module when the Library Browse route is first entered, rather than
adding WASM to application startup. It caches the module initializer and creates one
`LibraryBrowseCore` per route entry. `src/utils/createLibraryBrowseWindow.ts` translates virtualizer
display indexes into core inputs, executes emitted commands, stores page payloads in the UI query
cache, and renders the resulting items and placeholders.

Generated `wasm-pack` output lives under `crates/jellypilot-core-wasm/pkg/`. It is reproducible,
ignored build output, not hand-authored source. The stable generated module basename is
`jellypilot_core_wasm`; Rust source and the lockfile remain the reviewed inputs.

Sort direction is part of the server page request as `sortDirection: "asc" | "desc"`. Jellyfin and
Emby adapters map it to their native `Ascending`/`Descending` query value. The UI and core must not
reverse individual loaded pages to imitate global descending order.

## Consequences

- Browse policy has one deterministic implementation usable from WASM and native Rust.
- Rendering and transport remain replaceable adapters instead of leaking into the state machine.
- Tauri IPC remains a coarse page-data boundary; scroll/window updates stay local to the WebView.
- WASM initialization can fail independently of page loading. The active route uses its existing
  initial-error state, and the lazy loader must clear a rejected initializer so a later route entry
  can retry.
- Core, WASM boundary, adapter, rendered DOM, provider-wire, and native WebView behavior require
  separate regression coverage.
- Build and CI wrappers must regenerate and validate ignored WASM output before frontend typecheck
  or bundling consumes it.
