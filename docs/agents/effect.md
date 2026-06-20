## Scope

For generic Effect guidance, use these skills instead of duplicating policy here:

- `effect-ts`
- `effect-ts-extensions`

This document only records JMSR-specific frontend boundaries for Solid, TanStack Router, and typed Tauri IPC.

## JMSR Effect Boundaries

- Keep reusable frontend Tauri data workflows in `src/effects/**`.
- Wrap generated `commands.*` calls with `runTauriCommand` or `runTauriCommandRaw` from `src/effects/commands.ts`.
- Do not call raw `commands.*` from route components for reusable business/data workflows.
- Keep route `.tsx` files focused on Solid UI state, resource setup, boundary unwrapping, and rendering.
- Return plain success values from `src/effects/**`; do not fabricate success-shaped fallback objects for failures or missing values there.
- Keep fallback defaults at the Solid or TanStack boundary, not hidden inside `src/effects/**`.

## Route And UI Boundaries

- At Solid or TanStack boundaries, run Effect workflows and inspect `Exit`.
- Use `Exit.match`, `Exit.isSuccess`, or a small boundary helper such as `fetchThing().then(defaultTo(fallback))`.
- TanStack Router `throw redirect(...)` is allowed because the framework requires thrown redirect control flow.
- Test assertion helpers may throw when the test framework requires that shape.

## JMSR Review Checks

- [ ] Reusable Tauri IPC workflows live under `src/effects/**`.
- [ ] Route components do not call generated `commands.*` directly for reusable data workflows.
- [ ] `src/effects/**` reports failures or absence explicitly instead of returning fake fallback data.
- [ ] Fallback defaults are visible at the UI/router boundary.
- [ ] Framework-required throws stay local to the framework boundary.
