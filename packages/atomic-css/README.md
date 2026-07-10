# @jellypilot/atomic-css

Compile-time typed `atomic()` utilities for vanilla-extract.

## Public surface

- `@jellypilot/atomic-css` — `atomic()` marker
- `@jellypilot/atomic-css/preset-mini` — preset-mini adapter entry
- `@jellypilot/atomic-css/rsbuild` — `pluginAtomic()`
- `atomic-css` CLI — generate/check project types

Requires the Rsbuild plugin. An uncompiled marker throws:

```text
atomic() must be compiled by pluginAtomic()
```
