## Scope

For generic Effect guidance, use these skills instead of duplicating policy here:

- `effect-ts`
- `effect-ts-extensions`

This document is the single source of truth for JellyPilot's Effect rules. `AGENTS.md` points here; do not duplicate these rules there.

## Error Channel

- Never throw from business/data workflows; no raw `try/catch`. Use `Effect.fail` for recoverable failures, `Effect.try` / `Effect.tryPromise` to wrap throwing/rejecting APIs. Reserve throws for TanStack Router `throw redirect(...)` and test assertion helpers only.
- Use `Option` for meaningful absence that is still a successful value callers must compose or render. Use `Effect.fromNullishOr` (or an equivalent typed Effect failure) when missing data should short-circuit the workflow and be translated at the boundary. In reusable data workflows, convert nullable data to `Option` or Effect failure instead of ad hoc nullish checks; keep null checks that narrow unknown JSON/wire shapes.

## JellyPilot Effect Boundaries

- Keep reusable frontend Tauri data workflows in `src/effects/**`; route components call named workflows, never raw `commands.*`.
- Wrap generated `commands.*` calls with `runTauriCommand` or `runTauriCommandRaw` from `src/effects/commands.ts`.
- `src/effects/**` workflows return real success values, `Option` when absence is a successful value, or typed Effect failures when failure/absence should short-circuit. They must not fabricate success-shaped fallback objects. Keep fallback defaults at the Solid or TanStack boundary.
- Do not move Solid signal utilities such as `src/utils/createSharedLibraryFilters.ts` into `src/effects/**`; keep such utilities local when they own Solid signals while still using Effect wrappers internally.

## Route And UI Boundaries

- At Solid or TanStack boundaries, run Effect workflows and unwrap `Exit` with `Exit.match` / `Exit.isSuccess`, or a pipeline-style boundary helper. Use `Effect.runPromise` when a helper must preserve rejection semantics into an enclosing `Effect.tryPromise` or host promise boundary.
- When feeding an `Option`-unwrapped nullable value into a Solid `<Show>` callback, prefer a named derived accessor over complex inline `Option.getOrElse(...)` expressions in `Show when=` to avoid confusing Solid's generic inference.
- Framework-required throws are allowed at their boundaries: TanStack Router `throw redirect(...)` and test assertion helpers.

## Pattern Matching

- Use Effect's `Match` for value-based branching instead of `switch`. Reserve plain `switch` for framework or generated code where Effect is unavailable.
- Use `Match.exhaustive` for closed local/domain unions; `Match.orElse` for open/generated/stringly values that need a defensive fallback. Prefer module-level `Match` matchers for class-string and JSX-icon helpers so components call a stable function instead of rebuilding branch tables during render.
