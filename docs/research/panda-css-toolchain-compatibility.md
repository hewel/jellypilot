# Panda CSS toolchain compatibility

## Decision

JellyPilot can evaluate Panda CSS through Panda's documented Solid and Rsbuild/PostCSS path. Pin `@pandacss/dev` to exactly `1.11.4`; do not adopt the `2.0.0-beta.8` prerelease. This ticket establishes toolchain compatibility only and does not authorize dependency, configuration, or application-code changes.

The recommended integration is:

- install only `@pandacss/dev@1.11.4` for Panda;
- load `@pandacss/dev/postcss` from a root PostCSS configuration;
- add a root `panda.config.ts` with JellyPilot's `src` globs, `outdir: "styled-system"`, and `jsxFramework: "solid"`;
- declare Panda's cascade layers in the root stylesheet; and
- let Rsbuild/PostCSS perform extraction during normal development and production builds, without a second Panda watch process by default.

This recommendation is based on the stable package and its documented package entry points. The npm registry declares Node `>=20`, exports `./postcss`, and exposes the `panda` CLI; JellyPilot's Node requirement of `>=20.17` satisfies that floor. The registry currently distinguishes stable `1.11.4` from prerelease `2.0.0-beta.8`, so using a range would make the evaluation less reproducible. ([npm registry metadata](https://registry.npmjs.org/%40pandacss%2Fdev))

## Confirmed upstream behavior

### Solid and generated code

Panda's [Solid guide](https://panda-css.com/docs/installation/solidjs) scans Solid source files through `include`, generates a `styled-system` directory, declares Panda's CSS layers in the application stylesheet, and uses generated functions through Solid's `class` attribute, for example `class={css({ ... })}`. Panda's [configuration reference](https://panda-css.com/docs/references/config) supports `jsxFramework: "solid"` when generated JSX utilities are enabled. Conditional class composition should remain idiomatic Solid `classList`; Panda does not require a React-style `className` convention.

The generated styled system contains JavaScript modules and type declarations. Panda documents generated artifacts as reproducible output that is uncommitted by default, while allowing projects to choose another policy. ([styled-system generation](https://panda-css.com/docs/concepts/styled-system)) Panda's configuration reference documents MJS as the default generated JavaScript extension. ([configuration reference](https://panda-css.com/docs/references/config)) JellyPilot will therefore need the output directory included in TypeScript resolution once implementation begins.

The minimal prospective shape is:

```ts
// panda.config.ts (illustrative only)
import { defineConfig } from "@pandacss/dev"

export default defineConfig({
  include: ["./src/**/*.{js,jsx,ts,tsx}"],
  outdir: "styled-system",
  jsxFramework: "solid",
})
```

```css
@layer reset, base, tokens, recipes, utilities;
```

The exact preflight choice and layer ordering alongside existing styles are intentionally not decided here.

### Rsbuild and PostCSS

Panda's dedicated [Rsbuild guide](https://panda-css.com/docs/installation/rsbuild) uses `@pandacss/dev/postcss`; it does not require a Panda-specific Rsbuild plugin or another Panda package. Panda's generic [PostCSS guide](https://panda-css.com/docs/installation/postcss) describes the same plugin boundary. Rsbuild supports PostCSS 8 and automatically loads a conventional PostCSS configuration, so the configuration should live at the repository root where the existing build can discover it. ([Rsbuild PostCSS configuration](https://rsbuild.rs/config/tools/postcss))

Because Panda runs as a PostCSS plugin, Rsbuild remains the normal development and production CSS pipeline. The standalone `panda` CLI supports explicit code generation and watch modes, but an additional long-running Panda watcher should not be introduced unless the artifact-lifecycle prototype proves it necessary. ([Panda CLI](https://panda-css.com/docs/references/cli))

### Rstest, Bun, and Tauri boundaries

Rstest has its own test configuration and build-output controls, separate from Rsbuild's application build. ([Rstest getting started](https://rstest.rs/guide/start/), [Rstest build output](https://rstest.rs/config/build/output)) Panda generates ordinary ESM and declarations, so there is no documented Panda-specific Rstest plugin to install. This supports an inference—not yet an executed proof—that tests can import modules using generated Panda output after codegen. Rstest alone is not proof that production CSS was extracted or emitted.

Bun supports package lifecycle scripts, while dependency lifecycle scripts are restricted unless packages are trusted. ([Bun lifecycle scripts](https://bun.sh/docs/pm/lifecycle), [Bun install](https://bun.sh/docs/pm/cli/install)) JellyPilot already has a root `prepare` script for Husky. Whether Panda codegen belongs in `prepare`, another script, CI, or committed output is an artifact-lifecycle decision; this research does not modify or overload the existing hook.

JellyPilot's Tauri configuration runs `bun run build` as `beforeBuildCommand` and packages `../dist` as `frontendDist`. Therefore the Rsbuild production build, followed by a Tauri build/prototype, is the relevant proof that Panda CSS reaches the desktop bundle. Tauri's frontend guidance describes this build-command/output-directory contract. ([Tauri frontend build boundary](https://v2.tauri.app/start/frontend/vite/))

## Compatibility constraints

- **Static extraction:** Panda extracts statically analyzable style usage. Runtime-computed property names or arbitrary interpolated values must use supported dynamic-style patterns, CSS variables, or finite variants. ([dynamic styling](https://panda-css.com/docs/guides/dynamic-styling))
- **Variants:** Reusable finite variants should use Panda recipes rather than constructing dynamic class names. ([recipes](https://panda-css.com/docs/concepts/recipes))
- **Assets:** Panda's extraction model has documented limitations around asset URLs and build-tool asset handling. Asset-bearing declarations need prototype coverage rather than assuming that a source-relative URL will be rewritten correctly. ([Panda FAQ](https://panda-css.com/docs/overview/faq))
- **Cascade coexistence:** Panda's `reset`, `base`, `tokens`, `recipes`, and `utilities` layers must coexist deliberately with JellyPilot's current global and component styles. This research does not select the final preflight or cascade policy.
- **WebView floor:** Panda's supported browser baseline is Chrome 99, Firefox 97, Safari 15.4, and Edge 99. ([browser support](https://panda-css.com/docs/overview/browser-support)) Tauri uses OS-provided WebViews whose versions vary by platform and operating-system release, so the Safari/WebKit 15.4-equivalent floor remains a runtime compatibility gate. ([Tauri WebView versions](https://v2.tauri.app/reference/webview-versions/))

## JellyPilot-specific conclusions

The following are project inferences from the confirmed upstream behavior and the current repository configuration:

- `"type": "module"`, Node `>=20.17`, and Panda's default generated MJS output are compatible.
- A root PostCSS configuration is the narrowest integration seam shared by Rsbuild development and production builds.
- `panda.config.ts` should scan `src/**/*.{js,jsx,ts,tsx}`, generate to `styled-system`, and identify Solid explicitly.
- Rsbuild/PostCSS should be the default watcher. Explicit `panda codegen` remains useful for deterministic setup/CI, but exact script ownership is unresolved.
- Rstest should consume previously generated ESM without a Panda plugin. A successful Rsbuild/Tauri production path—not a unit test alone—must prove CSS extraction and desktop delivery.
- Tauri's existing `beforeBuildCommand -> bun run build -> dist` boundary does not need redesign for the evaluation.

These conclusions still require execution in the mapped prototype ticket; this document verifies that the official integration seams exist and fit the repository's declared toolchain.

## Deferred decisions

No new investigation ticket is needed. Existing mapped work already owns the remaining decisions:

- preflight, cascade-layer ordering, and coexistence policy belong to the Panda coexistence ticket;
- generated-artifact commit policy, codegen scripts, install/CI lifecycle, and watch behavior belong to the artifact-lifecycle ticket;
- token ownership and mapping belong to the token ticket; and
- final Solid authoring conventions, including conditional classes and recipe usage, belong to the Solid authoring ticket.

Until those tickets and the prototype are resolved, this document recommends the version and integration boundary but makes no claim that JellyPilot's full application has completed runtime validation.
