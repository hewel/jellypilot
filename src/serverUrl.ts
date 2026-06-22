export type ServerScheme = 'http' | 'https';

export interface ServerUrlFields {
  scheme: ServerScheme;
  host: string;
}

export interface ServerUrlResult {
  url: string;
  isLocal: boolean;
}

import { Effect, Exit } from 'effect';

import { InvalidServerUrl } from './effects/errors';

const LOCAL_HOSTS = new Set(['localhost']);

export function stripServerScheme(input: string): string {
  return input.trim().replace(/^https?:\/\//i, '');
}

export function explicitSchemeFromInput(input: string): ServerScheme | null {
  const match = input.trim().match(/^(https?):\/\//i);
  if (!match) {
    return null;
  }
  return match[1].toLowerCase() === 'http' ? 'http' : 'https';
}

function authorityFromHostInput(host: string): string {
  const slash = host.indexOf('/');
  return slash !== -1 ? host.slice(0, slash) : host;
}
export function hasExplicitPort(host: string): boolean {
  const authority = authorityFromHostInput(host);
  if (authority.startsWith('[')) {
    const end = authority.indexOf(']');
    return end !== -1 && authority.slice(end + 1).startsWith(':');
  }
  return authority.includes(':');
}

function hostWithoutPort(hostname: string): string {
  if (hostname.startsWith('[')) {
    const end = hostname.indexOf(']');
    return end !== -1 ? hostname.slice(1, end) : hostname;
  }
  const colon = hostname.indexOf(':');
  return colon !== -1 ? hostname.slice(0, colon) : hostname;
}

export function isLocalServerHost(hostname: string): boolean {
  const host = hostWithoutPort(hostname).toLowerCase();
  if (LOCAL_HOSTS.has(host) || host.endsWith('.local')) {
    return true;
  }
  if (host.startsWith('127.')) {
    return true;
  }
  if (host.startsWith('10.')) {
    return true;
  }
  if (host.startsWith('192.168.')) {
    return true;
  }

  const parts = host.split('.');
  if (parts.length === 4 && parts[0] === '172') {
    const second = Number(parts[1]);
    return Number.isInteger(second) && second >= 16 && second <= 31;
  }

  return false;
}

export function defaultSchemeForHost(host: string): ServerScheme {
  return isLocalServerHost(stripServerScheme(host)) ? 'http' : 'https';
}

export function buildServerUrlEffect(
  fields: ServerUrlFields,
): Effect.Effect<ServerUrlResult, InvalidServerUrl> {
  return Effect.gen(function* () {
    const rawHost = stripServerScheme(fields.host);
    if (!rawHost) {
      return yield* Effect.fail(new InvalidServerUrl({ message: 'Server host is required' }));
    }

    const candidate = `${fields.scheme}://${rawHost}`;
    const parsed = yield* Effect.try({
      catch: () =>
        new InvalidServerUrl({
          message: 'Enter a valid Jellyfin server host',
        }),
      try: () => new URL(candidate),
    });

    const isLocal = isLocalServerHost(parsed.host);
    const explicitPort = hasExplicitPort(rawHost);
    if (isLocal && !explicitPort) {
      parsed.port = '8096';
    }

    parsed.hash = '';
    parsed.search = '';

    const normalized = explicitPort
      ? `${fields.scheme}://${rawHost}`.replace(/[?#].*$/, '')
      : parsed.toString();

    const url = normalized.replace(/\/$/, parsed.pathname === '/' ? '' : '/');

    return { isLocal, url };
  });
}

export function buildServerUrl(fields: ServerUrlFields): ServerUrlResult {
  return Effect.runSync(buildServerUrlEffect(fields));
}

export function parseServerUrl(url: string | null | undefined): ServerUrlFields {
  if (!url) {
    return { scheme: 'https', host: '' };
  }

  const parsed = Effect.runSyncExit(
    Effect.try({ try: () => new URL(url), catch: (cause) => cause }),
  );
  if (Exit.isFailure(parsed)) {
    return { host: '', scheme: 'https' };
  }
  const scheme: ServerScheme = parsed.value.protocol === 'http:' ? 'http' : 'https';
  const host = `${parsed.value.host}${parsed.value.pathname === '/' ? '' : parsed.value.pathname}`;
  return { host, scheme };
}
