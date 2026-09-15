export type CargoWorkspace = 'shared' | 'desktop';

export const CARGO_MANIFESTS: Readonly<Record<CargoWorkspace, string>> = {
  shared: 'Cargo.toml',
  desktop: 'src-iced/Cargo.toml',
};

export type CrateShortName =
  | 'auth'
  | 'core'
  | 'media-server'
  | 'mpv'
  | 'mpv-host'
  | 'session'
  | 'sdk'
  | 'ffi'
  | 'iced';

export const CRATE_NAMES: Readonly<Record<CrateShortName, readonly string[]>> = {
  auth: ['jellypilot-auth'],
  core: ['jellypilot-core'],
  'media-server': ['jellypilot-media-server'],
  mpv: ['jellypilot-mpv'],
  'mpv-host': ['jellypilot-mpv-host'],
  session: ['jellypilot-session'],
  sdk: ['jellypilot-sdk'],
  ffi: ['jellypilot-ffi'],
  iced: ['jellypilot-ui', 'jellypilot-mpv-host', 'jellypilot-iced', 'jellypilot-launcher'],
};

const CRATE_WORKSPACES: Readonly<Record<CrateShortName, CargoWorkspace>> = {
  auth: 'shared',
  core: 'shared',
  'media-server': 'shared',
  mpv: 'shared',
  'mpv-host': 'desktop',
  session: 'shared',
  sdk: 'shared',
  ffi: 'shared',
  iced: 'desktop',
};

export function crateWorkspace(crate: CrateShortName): CargoWorkspace {
  return CRATE_WORKSPACES[crate];
}

// Distinct workspaces touched by a crate selection, in stable order. An empty
// selection means every maintained package and therefore both workspaces.
export function workspacesFor(crates: readonly CrateShortName[]): readonly CargoWorkspace[] {
  if (crates.length === 0) return ['shared', 'desktop'];
  const selected = new Set(crates.map(crateWorkspace));
  return (['shared', 'desktop'] as const).filter((workspace) => selected.has(workspace));
}

// Every maintained package in one workspace, derived from the shortname map so
// fmt/clippy coverage cannot drift from crate routing.
export function workspacePackages(workspace: CargoWorkspace): readonly string[] {
  const packages = new Set<string>();
  for (const crate of Object.keys(CRATE_NAMES) as readonly CrateShortName[]) {
    if (crateWorkspace(crate) !== workspace) continue;
    for (const packageName of CRATE_NAMES[crate]) packages.add(packageName);
  }
  return [...packages];
}

export function isCrateShortName(value: string): value is CrateShortName {
  return value in CRATE_NAMES;
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
