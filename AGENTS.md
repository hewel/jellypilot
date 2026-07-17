# AGENTS.md

Tauri v2 Jellyfin MPV Shim rewrite; external MPV controlled via JSON IPC (no libmpv embed).

## Role: Codex Planner + OMP Executor

- Keep Codex responsible for user intent, major direction, architecture, task decomposition, verification design, report review, and final user communication.
- For every repetitive, reversible, objectively verifiable slice with a closed write scope and exact checks, use the local `delegate-to-omp` skill to run a one-shot OMP worker through Herdr.
- OMP may edit only the assigned scope and run the verification methods selected by Codex. It must report blocked instead of widening scope, changing interfaces, or making a new major decision.
- Run at most one editing OMP task per repository. Parallelize read-only verification only when its evidence is independent of active edits.
- Treat native `@quick_task`, `@designer`, and `@oracle` agents as exceptional advisory or major-review lanes, not the default implementation path when a task qualifies for OMP delegation.
- After OMP reports, inspect its evidence and the resulting diff. Never recover, revert, or normalize user changes automatically.

## Stack

- **Frontend**: Solid.js + TypeScript + Rsbuild + TanStack Router + TanStack Form + Ark UI + Panda CSS
- **Backend**: Rust (Tauri v2) with tauri-specta for type-safe bindings
- **Data / Effects**: Effect-TS
- **Tools**: Bun, Oxc (Oxlint/Oxfmt), Rstest (jsdom)

## Where to Look

| Task | Location | Notes |
|------|----------|-------|
| Add Tauri command | `src-tauri/src/command.rs` | `#[tauri::command]` + `#[specta]` |
| Register command | `src-tauri/src/lib.rs` | Add to `collect_commands![]` |
| Frontend page | `src/routes/**` | Page-level `.tsx` route code; never split into `-*.tsx` modules |
| Reusable component | `src/components/**` | Non-page `.tsx` components; never in `src/features/**` |
| Tauri data workflow | `src/effects/**` | Reusable loaders/actions via Effect wrappers |
| Design tokens | `panda.config.ts` + `src/styles/theme-tokens.ts` | Panda owns all styling values |
| UI primitives | `src/components/ui/**` | Shared design-system components |
| Rust↔TS bindings | `src/bindings.ts` | Auto-generated (debug builds only); use typed `commands.*`, never raw `invoke()` |
| Add test | `tests/*.test.ts` | Rstest + @testing-library |
| Rust backend | `src-tauri/` | See `src-tauri/AGENTS.md` |
| Delegated OMP task | `.agents/skills/delegate-to-omp/` | Codex packet → Herdr socket → OMP RPC report |

## Commands

```bash
bun run dev          # Start Rsbuild dev server (port 3000)
bun run build        # Production build → dist/
bun run test         # Rstest
bun run test:watch   # Rstest watch mode
bun run check        # Oxfmt/Oxlint + type/Rust checks (includes format + typecheck)

# Tauri (run from project root)
bun tauri dev       # Dev mode with hot reload
bun tauri build     # Production desktop build
```

## Conventions

- **Solid.js**: Use the `solidjs` skill for all Solid-specific patterns.
- **Forms**: Always use `@tanstack/solid-form` with `createForm` for form handling.
- **Styling**: Panda CSS is the only styling system ([ADR 0011](docs/adr/0011-panda-css-styling.md)). Author owner-local `Component.styles.ts` / route `*.styles.ts` modules via `@styled-system/*`; use semantic Panda tokens and colocated `css` / `cva` / `sva`. Supported Tauri sizes: 1280×720 min, 1600×900 default. Stress review widths (review config only): 800×600, 640×720, 360×720. Design system: `docs/design-system.md`.
- **Solid classes**: Use `class` for static class strings and `classList` for conditional class maps. Do not add generic class-name merge helpers for Solid components.
- **TypeScript / Effect**: All Effect rules live in [docs/agents/effect.md](docs/agents/effect.md). Read and follow it; do not duplicate them here.
- **Route data loading**: Await only critical data; defer slow data as promises behind `<Suspense />` with stable skeletons. Follow [TanStack Router deferred data loading](https://tanstack.com/router/latest/docs/guide/deferred-data-loading).
- **Ark UI Dialogs**: Use standard Ark UI Dialog primitives; no custom ARIA overlays, `onInteractOutside` handlers, or `id` attributes. Controlled dialogs require `<Portal>`; use `lazyMount` + `unmountOnExit`.
- **Style Tests**: Style test policy in [docs/agents/style-tests.md](docs/agents/style-tests.md); read and follow it.

## Anti-Patterns

- **Cross-component private style imports**: Do not import another component's `.styles.ts` exports; consume the component API or move the needed behavior into a shared component.
- **Shared UI style barrels**: Do not create broad style barrels for unrelated components. Keep owner-local style modules beside their component or route.
- **Legacy styling**: Do not add `.css.ts`, vanilla-extract, Sprinkles, Recipes, UnoCSS, or authored `--jellypilot-*` variables.

## Agent Skills

- **OMP delegation**: Use `delegate-to-omp` for bounded implementation and verification tasks after Codex fixes scope, requirements, and exact checks.
- **Issue tracker**: GitHub Issues for `hewel/jmsr` — see [docs/agents/issue-tracker.md](docs/agents/issue-tracker.md)
- **Triage labels**: Five-label vocabulary — see [docs/agents/triage-labels.md](docs/agents/triage-labels.md)
- **Domain docs**: Root `CONTEXT.md` + `docs/adr/` — see [docs/agents/domain.md](docs/agents/domain.md)

## Docs

- Rsbuild: https://rsbuild.rs/llms.txt
- Rspack: https://rspack.rs/llms.txt
- Rstest: https://rstest.rs/llms.txt
- Tauri v2: https://v2.tauri.app
- tauri-specta: https://github.com/oscartbeaumont/tauri-specta
- Solid.js: https://docs.solidjs.com/
- Jellyfin API: https://api.jellyfin.org
