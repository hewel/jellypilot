export type ServerScheme = 'http' | 'https';

export interface ServerUrlFields {
  scheme: ServerScheme;
  host: string;
}

export interface ServerUrlResult {
  url: string;
  isLocal: boolean;
}

const LOCAL_HOSTS = new Set(['localhost']);

export function stripServerScheme(input: string): string {
  return input.trim().replace(/^https?:\/\//i, '');
}

export function explicitSchemeFromInput(input: string): ServerScheme | null {
  const match = input.trim().match(/^(https?):\/\//i);
  if (!match) return null;
  return match[1].toLowerCase() === 'http' ? 'http' : 'https';
}

function authorityFromHostInput(host: string): string {
  const slash = host.indexOf('/');
  return slash >= 0 ? host.slice(0, slash) : host;
}

function hasExplicitPort(host: string): boolean {
  const authority = authorityFromHostInput(host);
  if (authority.startsWith('[')) {
    const end = authority.indexOf(']');
    return end >= 0 && authority.slice(end + 1).startsWith(':');
  }
  return authority.includes(':');
}

function hostWithoutPort(hostname: string): string {
  if (hostname.startsWith('[')) {
    const end = hostname.indexOf(']');
    return end >= 0 ? hostname.slice(1, end) : hostname;
  }
  const colon = hostname.indexOf(':');
  return colon >= 0 ? hostname.slice(0, colon) : hostname;
}

export function isLocalServerHost(hostname: string): boolean {
  const host = hostWithoutPort(hostname).toLowerCase();
  if (LOCAL_HOSTS.has(host) || host.endsWith('.local')) return true;
  if (host.startsWith('127.')) return true;
  if (host.startsWith('10.')) return true;
  if (host.startsWith('192.168.')) return true;

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

export function buildServerUrl(fields: ServerUrlFields): ServerUrlResult {
  const rawHost = stripServerScheme(fields.host);
  if (!rawHost) {
    throw new Error('Server host is required');
  }

  const candidate = `${fields.scheme}://${rawHost}`;
  let parsed: URL;
  try {
    parsed = new URL(candidate);
  } catch {
    throw new Error('Enter a valid Jellyfin server host');
  }

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

  return {
    url: normalized.replace(/\/$/, parsed.pathname === '/' ? '' : '/'),
    isLocal,
  };
}

export function parseServerUrl(
  url: string | null | undefined,
): ServerUrlFields {
  if (!url) return { scheme: 'https', host: '' };

  try {
    const parsed = new URL(url);
    const scheme: ServerScheme = parsed.protocol === 'http:' ? 'http' : 'https';
    const host = `${parsed.host}${parsed.pathname === '/' ? '' : parsed.pathname}`;
    return { scheme, host };
  } catch {
    return { scheme: 'https', host: '' };
  }
}
