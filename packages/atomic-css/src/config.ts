import type { ThemeTokenRef } from './config/types.js'

/** Mark a Project Theme token as an intentional override of a preset token. */
export function overrideToken(
  family: string,
  name: string,
  value: string | ThemeTokenRef,
): { family: string; name: string; value: string | ThemeTokenRef } {
  return { family, name, value }
}

/** Serializable reference to a vanilla-extract theme export (not executed at load). */
export function themeRef(moduleId: string, exportName: string): ThemeTokenRef {
  return { kind: 'theme-ref', moduleId, exportName }
}
