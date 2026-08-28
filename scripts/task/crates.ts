export type CrateShortName =
  | 'auth'
  | 'core'
  | 'core-wasm'
  | 'media-server'
  | 'mpv'
  | 'session'
  | 'playback-core'
  | 'gtk'
  | 'iced';

export const CRATE_NAMES: Readonly<Record<CrateShortName, readonly string[]>> = {
  auth: ['jellypilot-auth'],
  core: ['jellypilot-core'],
  'core-wasm': ['jellypilot-core-wasm'],
  'media-server': ['jellypilot-media-server'],
  mpv: ['jellypilot-mpv'],
  session: ['jellypilot-session'],
  'playback-core': ['jellypilot-playback-core'],
  gtk: ['jellypilot-gtk'],
  iced: ['jellypilot-ui', 'jellypilot-iced'],
};

export function isCrateShortName(value: string): value is CrateShortName {
  return (
    value === 'auth' ||
    value === 'core' ||
    value === 'core-wasm' ||
    value === 'media-server' ||
    value === 'mpv' ||
    value === 'session' ||
    value === 'playback-core' ||
    value === 'gtk' ||
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
