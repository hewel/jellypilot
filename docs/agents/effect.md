## Scope

For generic Effect guidance, use these skills instead of duplicating policy here:

- `effect-ts`
- `effect-ts-extensions`

This document is the single source of truth for JellyPilot's Effect rules. `AGENTS.md` points here; do not duplicate these rules there.

## Error Channel

- Never throw `Error` values from business/data workflows. Use Effect's typed error channel: `Effect.fail` for recoverable failures, `Effect.try` / `Effect.tryPromise` to wrap throwing/rejecting APIs.
- Model optional values with `Option`, not nullish checks.
- No raw `try/catch` in TypeScript business/data workflows, and no nullish `if` checks in `src/effects/**`; model those paths with Effect and `Option`.
- Reserve thrown values for framework-required control flow only: TanStack Router `throw redirect(...)` and test assertion helpers.

## JellyPilot Effect Boundaries

- Keep reusable frontend Tauri data workflows in `src/effects/**`.
- Wrap generated `commands.*` calls with `runTauriCommand` or `runTauriCommandRaw` from `src/effects/commands.ts`.
- Do not call raw `commands.*` from route components for reusable business/data workflows.
- Keep route `.tsx` files focused on Solid UI state, resource setup, boundary unwrapping, and rendering.
- Return plain success values from `src/effects/**`; do not fabricate success-shaped fallback objects for failures or missing values there. Use Effect for failures, `Option` for nullable values, and return success values like empty arrays/pages as data.

## Route And UI Boundaries

- At Solid or TanStack boundaries, run Effect workflows and inspect `Exit`.
- Unwrap `Exit` with `Exit.match` / `Exit.isSuccess`, or a pipeline-style boundary helper such as `fetchThing().then(defaultTo(fallback))`.
- Keep fallback defaults at the Solid or TanStack boundary; do not hide command failures by fabricating success-shaped fallback objects inside `src/effects/**`.
- TanStack Router `throw redirect(...)` is allowed because the framework requires thrown redirect control flow.
- Test assertion helpers may throw when the test framework requires that shape.

## Pattern Matching

- For value-based branching in TypeScript, use Effect's `Match` (`Match.type<T>().pipe(Match.when(...), Match.orElse(...))`) instead of `switch` statements. It pipes through the Effect type system and keeps branching consistent with other Effect workflows.
- Reserve plain `switch` for framework or generated code where Effect is unavailable.

## JellyPilot Review Checks

- [ ] Reusable Tauri IPC workflows live under `src/effects/**`.
- [ ] Route components do not call generated `commands.*` directly for reusable data workflows.
- [ ] `src/effects/**` reports failures or absence explicitly instead of returning fake fallback data.
- [ ] No raw `try/catch` or nullish `if` checks in `src/effects/**`; failures use Effect, absence uses `Option`.
- [ ] Fallback defaults are visible at the UI/router boundary.
- [ ] Framework-required throws stay local to the framework boundary.
- [ ] Value-based branching uses Effect `Match`, not `switch`.
