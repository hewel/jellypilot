import type {
  Appearance,
  AppConfig,
  CommandError,
  ConnectionState,
  Credentials,
  NowPlayingState,
  OpaqueCanvasRgb,
  SavedServiceProfiles,
  VideoHome,
  VideoItemDetail,
  VideoLibraryPage,
  VideoLibraryShortcut,
  VideoSearchPage,
} from '../../src/bindings';

export const FIXTURE_PASSWORD = 'not-a-secret';
export const FIXTURE_NETWORK_ERROR = {
  code: 'network',
  message: 'E2E fixture: server unreachable',
} as const satisfies CommandError;

export const EXPECTED_CREDENTIALS = {
  provider: 'jellyfin',
  serverUrl: 'https://media.invalid',
  username: 'e2e-user',
  password: FIXTURE_PASSWORD,
} as const satisfies Credentials;

interface RawCommandMap {
  appearance_get: Appearance;
  appearance_ready: null;
  appearance_set: null;
  config_default: unknown;
  config_get: AppConfig;
  library_browse_video: VideoLibraryPage;
  library_item_detail: VideoItemDetail;
  library_item_shortcut: VideoLibraryShortcut | null;
  library_play: null;
  library_search_video: VideoSearchPage;
  library_video_home: VideoHome;
  library_video_shortcuts: VideoLibraryShortcut[];
  mpv_is_connected: boolean;
  now_playing_get_state: NowPlayingState;
  'plugin:window|is_visible': boolean;
  'plugin:window|theme': string | null;
  server_connect: null;
  server_get_state: ConnectionState;
  server_is_connected: boolean;
  server_profiles_activate: SavedServiceProfiles;
  server_profiles_get: SavedServiceProfiles;
  server_profiles_reauthenticate_password: SavedServiceProfiles;
}

export type FixtureCommand = keyof RawCommandMap;
export type SafeRealCommand =
  | 'appearance_get'
  | 'appearance_ready'
  | 'appearance_set'
  | 'config_default'
  | 'plugin:window|is_visible'
  | 'plugin:window|theme';

export type FixtureOutcome<C extends FixtureCommand = FixtureCommand> =
  | { readonly kind: 'return'; readonly value: RawCommandMap[C] }
  | { readonly kind: 'error'; readonly error: CommandError }
  | { readonly kind: 'real' };

type InvokeArgs = Record<string, unknown> | undefined;
type RealInvoke = <T>(command: string, args?: InvokeArgs) => Promise<T>;
type StoredFixtureOutcome =
  | { readonly kind: 'real' }
  | { readonly kind: 'error'; readonly error: CommandError }
  | {
      [C in FixtureCommand]: { readonly kind: 'return'; readonly value: RawCommandMap[C] };
    }[FixtureCommand];

export const DEFAULT_APPEARANCE = {
  designTheme: 'controlRoom',
  colorMode: 'dark',
} as const satisfies Appearance;

export const DEFAULT_APPEARANCE_CANVAS = {
  red: 5,
  green: 6,
  blue: 10,
} as const satisfies OpaqueCanvasRgb;

const safeRealCommands = new Set<SafeRealCommand>([
  'appearance_get',
  'appearance_ready',
  'appearance_set',
  'config_default',
  'plugin:window|is_visible',
  'plugin:window|theme',
]);
const fixtures = new Map<FixtureCommand, StoredFixtureOutcome>();
const calls = new Map<FixtureCommand, InvokeArgs[]>();

function parseFixtureCommand(command: string): FixtureCommand | undefined {
  if (
    command === 'appearance_get' ||
    command === 'appearance_ready' ||
    command === 'appearance_set' ||
    command === 'config_default' ||
    command === 'config_get' ||
    command === 'library_browse_video' ||
    command === 'library_item_detail' ||
    command === 'library_item_shortcut' ||
    command === 'library_play' ||
    command === 'library_search_video' ||
    command === 'library_video_home' ||
    command === 'library_video_shortcuts' ||
    command === 'mpv_is_connected' ||
    command === 'now_playing_get_state' ||
    command === 'plugin:window|is_visible' ||
    command === 'plugin:window|theme' ||
    command === 'server_connect' ||
    command === 'server_get_state' ||
    command === 'server_is_connected' ||
    command === 'server_profiles_activate' ||
    command === 'server_profiles_get' ||
    command === 'server_profiles_reauthenticate_password'
  ) {
    return command;
  }
  return undefined;
}

function recordCall(command: FixtureCommand, args: InvokeArgs): void {
  const commandCalls = calls.get(command) ?? [];
  commandCalls.push(args);
  calls.set(command, commandCalls);
}

export function installStartupFixtures(): void {
  fixtures.clear();
  calls.clear();
  fixtures.set('server_is_connected', { kind: 'return', value: false });
  fixtures.set('server_profiles_get', {
    kind: 'return',
    value: { activeProfileKey: null, profiles: [] },
  });
  fixtures.set('server_connect', { kind: 'error', error: FIXTURE_NETWORK_ERROR });
  fixtures.set('config_default', { kind: 'real' });
  fixtures.set('appearance_get', { kind: 'return', value: DEFAULT_APPEARANCE });
  fixtures.set('appearance_ready', { kind: 'real' });
  fixtures.set('appearance_set', { kind: 'real' });
  fixtures.set('plugin:window|is_visible', { kind: 'real' });
  fixtures.set('plugin:window|theme', { kind: 'real' });
}

export function installFixture<C extends FixtureCommand>(
  command: C,
  outcome: FixtureOutcome<C>,
): void {
  fixtures.set(command, outcome);
  calls.delete(command);
}

export function createControlledInvoke(realInvoke: RealInvoke): RealInvoke {
  return async <T>(command: string, args?: InvokeArgs): Promise<T> => {
    const fixtureCommand = parseFixtureCommand(command);
    if (!fixtureCommand) {
      throw new Error(`Rejected undeclared E2E IPC command: ${command}`);
    }

    recordCall(fixtureCommand, args);
    const outcome = fixtures.get(fixtureCommand);
    if (!outcome) throw new Error(`Missing E2E fixture outcome: ${command}`);

    if (outcome.kind === 'return') return outcome.value as T;
    if (outcome.kind === 'error') throw outcome.error;
    if (!safeRealCommands.has(fixtureCommand as SafeRealCommand)) {
      throw new Error(`Rejected unsafe real E2E IPC command: ${command}`);
    }

    return realInvoke<T>(command, args);
  };
}

export function fixtureCallCount(command: FixtureCommand): number {
  return calls.get(command)?.length ?? 0;
}

export function fixtureSummary(): readonly { command: FixtureCommand; count: number }[] {
  return [...fixtures.keys()].map((command) => ({ command, count: fixtureCallCount(command) }));
}

export function hasExpectedLibraryPlayCall(): boolean {
  const commandCalls = calls.get('library_play');
  if (!commandCalls || commandCalls.length !== 1) return false;

  const request = commandCalls[0]?.request;
  if (!request || typeof request !== 'object') return false;

  return (
    'itemId' in request &&
    request.itemId === 'e2e-home-movie' &&
    'mode' in request &&
    request.mode === 'resume' &&
    'startPositionSeconds' in request &&
    request.startPositionSeconds === 120 &&
    'audioStreamIndex' in request &&
    request.audioStreamIndex === null &&
    'subtitleStreamIndex' in request &&
    request.subtitleStreamIndex === null
  );
}

export function hasExpectedReauthenticatePasswordCall(expectedKey: string): boolean {
  const commandCalls = calls.get('server_profiles_reauthenticate_password');
  if (!commandCalls || commandCalls.length !== 1) return false;

  const args = commandCalls[0];
  if (!args || typeof args !== 'object') return false;

  return (
    'key' in args &&
    args.key === expectedKey &&
    'password' in args &&
    args.password === FIXTURE_PASSWORD
  );
}

export function hasExpectedServerConnectCall(): boolean {
  const commandCalls = calls.get('server_connect');
  if (!commandCalls || commandCalls.length !== 1) return false;

  const credentials = commandCalls[0]?.credentials;
  if (!credentials || typeof credentials !== 'object') return false;
  return (
    'provider' in credentials &&
    credentials.provider === EXPECTED_CREDENTIALS.provider &&
    'serverUrl' in credentials &&
    credentials.serverUrl === EXPECTED_CREDENTIALS.serverUrl &&
    'username' in credentials &&
    credentials.username === EXPECTED_CREDENTIALS.username &&
    'password' in credentials &&
    credentials.password === EXPECTED_CREDENTIALS.password
  );
}

export function hasExpectedAppearanceReadyCall(
  expectedAppearance: Appearance,
  expectedCanvas: OpaqueCanvasRgb,
): boolean {
  const commandCalls = calls.get('appearance_ready');
  if (!commandCalls || commandCalls.length !== 1) return false;

  const request = commandCalls[0]?.request;
  if (!request || typeof request !== 'object') return false;

  const appearance = 'appearance' in request ? request.appearance : undefined;
  const canvas = 'canvas' in request ? request.canvas : undefined;
  if (!appearance || typeof appearance !== 'object') return false;
  if (!canvas || typeof canvas !== 'object') return false;

  return (
    'designTheme' in appearance &&
    appearance.designTheme === expectedAppearance.designTheme &&
    'colorMode' in appearance &&
    appearance.colorMode === expectedAppearance.colorMode &&
    'red' in canvas &&
    canvas.red === expectedCanvas.red &&
    'green' in canvas &&
    canvas.green === expectedCanvas.green &&
    'blue' in canvas &&
    canvas.blue === expectedCanvas.blue
  );
}

export function hasExpectedAppearanceSetCall(
  expectedAppearance: Appearance,
  expectedCanvas: OpaqueCanvasRgb,
  options: { readonly exactlyOnce?: boolean } = {},
): boolean {
  const commandCalls = calls.get('appearance_set');
  if (!commandCalls || commandCalls.length === 0) return false;
  if (options.exactlyOnce && commandCalls.length !== 1) return false;

  return commandCalls.some((args) => {
    const request = args?.request;
    if (!request || typeof request !== 'object') return false;

    const appearance = 'appearance' in request ? request.appearance : undefined;
    const canvas = 'canvas' in request ? request.canvas : undefined;
    if (!appearance || typeof appearance !== 'object') return false;
    if (!canvas || typeof canvas !== 'object') return false;

    return (
      'designTheme' in appearance &&
      appearance.designTheme === expectedAppearance.designTheme &&
      'colorMode' in appearance &&
      appearance.colorMode === expectedAppearance.colorMode &&
      'red' in canvas &&
      canvas.red === expectedCanvas.red &&
      'green' in canvas &&
      canvas.green === expectedCanvas.green &&
      'blue' in canvas &&
      canvas.blue === expectedCanvas.blue
    );
  });
}

export function appearanceSetCalls(): readonly InvokeArgs[] {
  return calls.get('appearance_set') ?? [];
}

installStartupFixtures();
