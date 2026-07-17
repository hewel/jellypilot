import type { CommandError, Credentials, SavedServiceProfiles } from '../../src/bindings';

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
  config_default: unknown;
  server_connect: null;
  server_is_connected: boolean;
  server_profiles_get: SavedServiceProfiles;
}

export type FixtureCommand = keyof RawCommandMap;
export type SafeRealCommand = 'config_default';

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

const safeRealCommands = new Set<SafeRealCommand>(['config_default']);
const fixtures = new Map<FixtureCommand, StoredFixtureOutcome>();
const calls = new Map<FixtureCommand, InvokeArgs[]>();

function parseFixtureCommand(command: string): FixtureCommand | undefined {
  if (
    command === 'config_default' ||
    command === 'server_connect' ||
    command === 'server_is_connected' ||
    command === 'server_profiles_get'
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
    if (fixtureCommand !== 'config_default' || !safeRealCommands.has(fixtureCommand)) {
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
