## Scope

For generic Effect guidance, use these skills instead of duplicating policy here:

- `effect-ts`
- `effect-ts-extensions`

This document is the single source of truth for JellyPilot's Effect rules. `AGENTS.md` points here; do not duplicate these rules there.

## Error Channel

- Never throw `Error` values from business/data workflows. Use Effect's typed error channel: `Effect.fail` for recoverable failures, `Effect.try` / `Effect.tryPromise` to wrap throwing/rejecting APIs.
- Use `Option` for meaningful absence that is still a successful value callers must compose or render, for example MPV path detection (`src/effects/config.ts#detectMpv`), normalized media-detail fields (`src/effects/library.ts#MediaDetail`), selected seasons (`src/effects/library.ts#initialSeasonForShow`), command-failure extraction (`src/effects/commands.ts#commandFailure`), and parsed store snapshots (`src/utils/createSharedLibraryFilters.ts#parseStoreSnapshot`).
- Use `Effect.fromNullishOr` (or an equivalent typed Effect failure) when missing data should short-circuit the workflow and be translated at the boundary, for example saved-session (`src/effects/auth.ts#loadSavedSession`) and saved-credentials (`src/effects/session.ts#loadSavedCredentials`) loaders that simply skip restore/prefill when nothing is stored.
- No raw `try/catch` in TypeScript business/data workflows. In reusable data workflows, do not branch on nullable storage/IPC data with ad hoc nullish checks; convert it to `Option` when absence is a success path or to an Effect failure when absence should short-circuit. Keep null checks that narrow unknown JSON/wire shapes.
- Reserve thrown values for framework-required control flow only: TanStack Router `throw redirect(...)` and test assertion helpers.

## JellyPilot Effect Boundaries

- Keep reusable frontend Tauri data workflows in `src/effects/**`.
- Wrap generated `commands.*` calls with `runTauriCommand` or `runTauriCommandRaw` from `src/effects/commands.ts`.
- When a UI component needs reusable IPC, add or reuse a named workflow under `src/effects/**` first, then call that workflow from the component; example: `LoginPage` calls `connectJellyfin` from `src/effects/connection.ts` instead of wrapping `commands.jellyfinConnect` inline.
- Do not call raw `commands.*` from route components for reusable business/data workflows.
- Keep route `.tsx` files focused on Solid UI state, resource setup, boundary unwrapping, and rendering.
- `src/effects/**` workflows return real success values, `Option` when absence is a successful value, or typed Effect failures when failure/absence should short-circuit. They must not fabricate success-shaped fallback objects for command failures or missing required data. Domain-valid empty collections/pages are still allowed when the command actually succeeded.
- Do not move Solid signal utilities such as `src/utils/createSharedLibraryFilters.ts` into `src/effects/**`; keep such utilities local when they own Solid signals while still using Effect wrappers internally.

## Route And UI Boundaries

- At Solid or TanStack boundaries, run Effect workflows and inspect `Exit`.
- Use `Effect.runPromise` instead when a helper must preserve rejection semantics into an enclosing `Effect.tryPromise` or host promise boundary; example: `src/sessionAccess.ts#saveCurrentSession` propagates a failed `jellyfinGetSession` so `LoginPage` keeps showing the existing session-save error.
- Unwrap `Exit` with `Exit.match` / `Exit.isSuccess`, or a pipeline-style boundary helper such as `fetchThing().then(defaultTo(fallback))`. `Exit.match`, `Option.getOrNull`, `Option.getOrElse`, and small local accessors are boundary unwrapping tools, not core-workflow fallbacks.
- When feeding an `Option`-unwrapped nullable value into a Solid `<Show>` callback, prefer a named derived accessor such as `const overviewText = () => Option.getOrElse(detail.overview, () => null);` then `<Show when={overviewText()}>` (as in `src/components/library/MediaInfoHoverCard.tsx#MediaInfoContent`). Avoid complex inline `Option.getOrElse(...)` expressions in `Show when=` because they can confuse Solid's generic inference.
- Keep fallback defaults at the Solid or TanStack boundary; do not hide command failures by fabricating success-shaped fallback objects inside `src/effects/**`.
- TanStack Router `throw redirect(...)` is allowed because the framework requires thrown redirect control flow.
- Test assertion helpers may throw when the test framework requires that shape.

## Pattern Matching

- For value-based branching in TypeScript, use Effect's `Match` (`Match.type<T>().pipe(Match.when(...), Match.orElse(...))`) instead of `switch` statements. It pipes through the Effect type system and keeps branching consistent with other Effect workflows.
- Use `Match.exhaustive` for closed local/domain unions when every member is intentionally handled, for example `StatusBadgeVariant` (`src/components/ui/StatusBadge.tsx#variantClassMatcher`). Use `Match.orElse` for open/generated/stringly values that need a defensive fallback, for example notification levels, playback status labels, unavailable reason strings, and persisted filter parser inputs.
- Prefer module-level `Match` matchers for class-string and JSX-icon helpers so components call a stable function instead of rebuilding branch tables during render.
- Reserve plain `switch` for framework or generated code where Effect is unavailable.

## JellyPilot Review Checks

- [ ] Reusable Tauri IPC workflows live under `src/effects/**`.
- [ ] Route components do not call generated `commands.*` directly for reusable data workflows; they call a named `src/effects/**` workflow.
- [ ] Absence is explicit: `Option` for successful optional values, or a typed Effect failure when missing data should short-circuit and be translated at the boundary.
- [ ] Reusable workflows do not use raw `try/catch`, ad hoc nullable branching, or fake success-shaped fallbacks; JSON/wire-shape null checks are limited to narrowing unknown external data.
- [ ] Boundaries unwrap with `Exit` / `Option` helpers when rendering or deciding fallback state, and use `runPromise` only when intentionally preserving a rejection into an enclosing promise/Effect boundary.
- [ ] Fallback defaults are visible at the UI/router boundary.
- [ ] Framework-required throws stay local to the framework boundary.
- [ ] Closed unions use `Match.exhaustive`; open/generated value branches use `Match.orElse` with the existing user-visible fallback.
- [ ] Value-based branching uses Effect `Match`, not `switch`.
