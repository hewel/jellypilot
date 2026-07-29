import type {
  AppLocalServices,
  AppConfig,
  CommandError,
  ConnectionState,
  Credentials,
  NowPlayingState,
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
  app_local_services: AppLocalServices;
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
  server_connect: null;
  server_get_state: ConnectionState;
  server_is_connected: boolean;
  server_profiles_activate: SavedServiceProfiles;
  server_profiles_get: SavedServiceProfiles;
  server_profiles_reauthenticate_password: SavedServiceProfiles;
}

export type FixtureCommand = keyof RawCommandMap;
export type SafeRealCommand = 'app_local_services' | 'config_default';

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

const safeRealCommands = new Set<SafeRealCommand>(['app_local_services', 'config_default']);
const fixtures = new Map<FixtureCommand, StoredFixtureOutcome>();
const calls = new Map<FixtureCommand, InvokeArgs[]>();

function parseFixtureCommand(command: string): FixtureCommand | undefined {
  if (
    command === 'app_local_services' ||
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
  fixtures.set('app_local_services', { kind: 'real' });
  fixtures.set('server_profiles_get', {
    kind: 'return',
    value: { activeProfileKey: null, profiles: [] },
  });
  fixtures.set('server_connect', { kind: 'error', error: FIXTURE_NETWORK_ERROR });
  fixtures.set('config_default', { kind: 'real' });
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
    if (
      (fixtureCommand !== 'app_local_services' && fixtureCommand !== 'config_default') ||
      !safeRealCommands.has(fixtureCommand)
    ) {
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

installStartupFixtures();
