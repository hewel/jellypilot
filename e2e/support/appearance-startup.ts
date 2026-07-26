import type { Appearance, OpaqueCanvasRgb } from '../../src/bindings';

export const APPEARANCE_STARTUP_CASE_IDS = [
  'control-room-dark',
  'control-room-light',
  'braun-dark',
  'braun-light',
] as const;

export type AppearanceStartupCaseId = (typeof APPEARANCE_STARTUP_CASE_IDS)[number];

export interface AppearanceStartupCase {
  readonly id: AppearanceStartupCaseId;
  readonly appearance: Appearance;
  readonly pandaTheme: 'control-room' | 'braun';
  readonly colorMode: 'light' | 'dark';
  readonly nativeTheme: 'light' | 'dark';
  readonly canvas: OpaqueCanvasRgb;
  readonly canvasCss: string;
}

export const APPEARANCE_STARTUP_CASES: readonly AppearanceStartupCase[] = [
  {
    id: 'control-room-dark',
    appearance: { designTheme: 'controlRoom', colorMode: 'dark' },
    pandaTheme: 'control-room',
    colorMode: 'dark',
    nativeTheme: 'dark',
    canvas: { red: 5, green: 6, blue: 10 },
    canvasCss: 'rgb(5, 6, 10)',
  },
  {
    id: 'control-room-light',
    appearance: { designTheme: 'controlRoom', colorMode: 'light' },
    pandaTheme: 'control-room',
    colorMode: 'light',
    nativeTheme: 'light',
    canvas: { red: 246, green: 247, blue: 255 },
    canvasCss: 'rgb(246, 247, 255)',
  },
  {
    id: 'braun-dark',
    appearance: { designTheme: 'braun', colorMode: 'dark' },
    pandaTheme: 'braun',
    colorMode: 'dark',
    nativeTheme: 'dark',
    canvas: { red: 12, green: 14, blue: 18 },
    canvasCss: 'rgb(12, 14, 18)',
  },
  {
    id: 'braun-light',
    appearance: { designTheme: 'braun', colorMode: 'light' },
    pandaTheme: 'braun',
    colorMode: 'light',
    nativeTheme: 'light',
    canvas: { red: 252, green: 248, blue: 248 },
    canvasCss: 'rgb(252, 248, 248)',
  },
] as const;

export const APPEARANCE_STARTUP_ENV = 'JELLYPILOT_E2E_APPEARANCE_CASE';

/** Linux app-data path used by the webdriver Tauri identifier for tauri-plugin-store. */
export const E2E_APP_IDENTIFIER = 'top.pigfun.jellypilot.webdriver';

export function isAppearanceStartupCaseId(value: string): value is AppearanceStartupCaseId {
  return (APPEARANCE_STARTUP_CASE_IDS as readonly string[]).includes(value);
}

export function appearanceStartupCaseById(id: AppearanceStartupCaseId): AppearanceStartupCase {
  const match = APPEARANCE_STARTUP_CASES.find((entry) => entry.id === id);
  if (!match) {
    throw new Error(`Unknown appearance startup case: ${id}`);
  }
  return match;
}

export function appearanceStartupSpecPath(specAbsolutePath: string): boolean {
  return specAbsolutePath.replaceAll('\\', '/').endsWith('/e2e/specs/appearance-startup.e2e.ts');
}

export function expandAppearanceStartupRuns(specs: readonly string[]): readonly {
  readonly spec: string;
  readonly appearanceCaseId?: AppearanceStartupCaseId;
}[] {
  const runs: { readonly spec: string; readonly appearanceCaseId?: AppearanceStartupCaseId }[] = [];
  for (const spec of specs) {
    if (appearanceStartupSpecPath(spec)) {
      for (const appearanceCase of APPEARANCE_STARTUP_CASES) {
        runs.push({ spec, appearanceCaseId: appearanceCase.id });
      }
      continue;
    }
    runs.push({ spec });
  }
  return runs;
}

export function linuxAppearanceStorePath(xdgDataHome: string): string {
  return `${xdgDataHome.replace(/\/$/u, '')}/${E2E_APP_IDENTIFIER}/config.json`;
}

export function appearanceStoreDocument(appearance: Appearance): {
  readonly app_config: { readonly appearance: Appearance };
} {
  return {
    app_config: {
      appearance,
    },
  };
}
