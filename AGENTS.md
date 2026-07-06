# AGENTS.md

Tauri v2 Jellyfin MPV Shim rewrite; external MPV controlled via JSON IPC (no libmpv embed).

## Role: Task Router

Route incoming requests to the correct agent using these strict criteria:

- @quick_task: Git operations (commit, push, branch, etc.)
- @designer  : Client/Frontend styling, CSS, UI/UX.
- @oracle    : Backend code, APIs, and reviewing **major changes** only (skip for small ones).

## Stack

- **Frontend**: Solid.js + TypeScript + Rsbuild + TanStack Router + TanStack Form + Ark UI + Tailwind + vanilla-extract
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
| Design tokens | `src/styles/vars.css.ts` | vanilla-extract design tokens |
| UI primitives | `src/components/ui/**` | Shared design-system components |
| Rust↔TS bindings | `src/bindings.ts` | Auto-generated (debug builds only); use typed `commands.*`, never raw `invoke()` |
| Add test | `tests/*.test.ts` | Rstest + @testing-library |
| Rust backend | `src-tauri/` | See `src-tauri/AGENTS.md` |

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
- **Styling**: Tailwind for migration-era atomic styling; vanilla-extract for tokens (`src/styles/vars.css.ts`), Sprinkles (`src/styles/sprinkles.css.ts`), component Recipes, and complex CSS. Compose static vanilla-extract classes with `style([sprinkles(...), { ... }])`, not manual `.join(' ')` class strings. Component-owned classes live beside the component in `Component.css.ts`; do not collect unrelated UI component styles into a shared `styles.css.ts` barrel. No global `@layer components`; create or extend components in `src/components/ui`. Design system: `docs/design-system.md`.
- **Solid classes**: Use `class` for static class strings and `classList` for conditional class maps. Do not add generic class-name merge helpers for Solid components.
- **TypeScript / Effect**: All Effect rules live in [docs/agents/effect.md](docs/agents/effect.md). Read and follow it; do not duplicate them here.
- **Route data loading**: Await only critical data; defer slow data as promises behind `<Suspense />` with stable skeletons. Follow [TanStack Router deferred data loading](https://tanstack.com/router/latest/docs/guide/deferred-data-loading).
- **Ark UI Dialogs**: Use standard Ark UI Dialog primitives; no custom ARIA overlays, `onInteractOutside` handlers, or `id` attributes. Controlled dialogs require `<Portal>`; use `lazyMount` + `unmountOnExit`.
- **Style Tests**: Style test policy in [docs/agents/style-tests.md](docs/agents/style-tests.md); read and follow it.

## Anti-Patterns

- **Cross-component `.css.ts` imports**: Do not import another component's `.css.ts` style exports directly; consume the component API or move the needed behavior into a shared component.
- **Shared UI style barrels**: Do not create broad files such as `src/components/ui/styles.css.ts` that export styles for many unrelated components. Split component-owned styles into each component's own `.css.ts` file.

## Agent Skills

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
