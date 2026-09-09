export type CrateShortName =
  | 'auth'
  | 'core'
  | 'media-server'
  | 'mpv'
  | 'mpv-host'
  | 'session'
  | 'iced';

export const CRATE_NAMES: Readonly<Record<CrateShortName, readonly string[]>> = {
  auth: ['jellypilot-auth'],
  core: ['jellypilot-core'],
  'media-server': ['jellypilot-media-server'],
  mpv: ['jellypilot-mpv'],
  'mpv-host': ['jellypilot-mpv-host'],
  session: ['jellypilot-session'],
  iced: ['jellypilot-ui', 'jellypilot-mpv-host', 'jellypilot-iced', 'jellypilot-launcher'],
};

export function isCrateShortName(value: string): value is CrateShortName {
  return (
    value === 'auth' ||
    value === 'core' ||
    value === 'media-server' ||
    value === 'mpv' ||
    value === 'mpv-host' ||
    value === 'session' ||
    value === 'iced'
  );
}

function parseCrate(value: string): CrateShortName {
  if (!isCrateShortName(value)) {
    throw new Error(`Unknown crate '${value}'.`);
  }
  return value;
}

export function resolveCrates(value: string): readonly string[] {
  return CRATE_NAMES[parseCrate(value)];
}

export function parseCrates(values: readonly string[]): readonly CrateShortName[] {
  return values.map(parseCrate);
}
