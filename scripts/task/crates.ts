export type CrateShortName =
  | 'core'
  | 'core-wasm'
  | 'media-server'
  | 'mpv'
  | 'session'
  | 'playback-core'
  | 'gtk';

export const CRATE_NAMES: Readonly<Record<CrateShortName, string>> = {
  core: 'jellypilot-core',
  'core-wasm': 'jellypilot-core-wasm',
  'media-server': 'jellypilot-media-server',
  mpv: 'jellypilot-mpv',
  session: 'jellypilot-session',
  'playback-core': 'jellypilot-playback-core',
  gtk: 'jellypilot-gtk',
};

export function isCrateShortName(value: string): value is CrateShortName {
  return (
    value === 'core' ||
    value === 'core-wasm' ||
    value === 'media-server' ||
    value === 'mpv' ||
    value === 'session' ||
    value === 'playback-core' ||
    value === 'gtk'
  );
}

function parseCrate(value: string): CrateShortName {
  if (!isCrateShortName(value)) {
    throw new Error(`Unknown crate '${value}'.`);
  }
  return value;
}

export function resolveCrate(value: string): string {
  return CRATE_NAMES[parseCrate(value)];
}

export function parseCrates(values: readonly string[]): readonly CrateShortName[] {
  return values.map(parseCrate);
}
