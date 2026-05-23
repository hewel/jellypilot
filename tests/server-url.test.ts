import { expect, test } from '@rstest/core';
import { Cause, Effect, Exit } from 'effect';
import { InvalidServerUrl } from '../src/effects/errors';
import { buildServerUrlEffect } from '../src/effects/serverUrl';
import {
  buildServerUrl,
  defaultSchemeForHost,
  parseServerUrl,
} from '../src/serverUrl';

test('local hosts default to http', () => {
  expect(defaultSchemeForHost('localhost')).toBe('http');
  expect(defaultSchemeForHost('jellyfin.local')).toBe('http');
  expect(defaultSchemeForHost('192.168.1.20')).toBe('http');
  expect(defaultSchemeForHost('10.0.0.4')).toBe('http');
  expect(defaultSchemeForHost('172.20.0.4')).toBe('http');
});

test('public hosts default to https', () => {
  expect(defaultSchemeForHost('media.example.com')).toBe('https');
});

test('adds jellyfin port only for local hosts without explicit port', () => {
  expect(buildServerUrl({ scheme: 'http', host: '192.168.1.20' }).url).toBe(
    'http://192.168.1.20:8096',
  );
  expect(
    buildServerUrl({ scheme: 'https', host: 'media.example.com' }).url,
  ).toBe('https://media.example.com');
});

test('preserves explicit ports and path prefixes', () => {
  expect(
    buildServerUrl({ scheme: 'https', host: 'media.example.com:443/jellyfin' })
      .url,
  ).toBe('https://media.example.com:443/jellyfin');
  expect(
    buildServerUrl({ scheme: 'http', host: 'jellyfin.local:8097/base' }).url,
  ).toBe('http://jellyfin.local:8097/base');
  expect(
    buildServerUrl({
      scheme: 'https',
      host: 'http://media.example.com:8080/base',
    }).url,
  ).toBe('https://media.example.com:8080/base');
});

test('buildServerUrlEffect preserves normalized server url outputs', () => {
  const local = Effect.runSync(
    buildServerUrlEffect({ scheme: 'http', host: '192.168.1.20' }),
  );
  expect(local).toEqual({
    url: 'http://192.168.1.20:8096',
    isLocal: true,
  });

  const reverseProxy = Effect.runSync(
    buildServerUrlEffect({
      scheme: 'https',
      host: 'media.example.com:443/jellyfin',
    }),
  );
  expect(reverseProxy).toEqual({
    url: 'https://media.example.com:443/jellyfin',
    isLocal: false,
  });
});

test('parses saved final urls into visible fields', () => {
  expect(parseServerUrl('http://192.168.1.20:8096')).toEqual({
    scheme: 'http',
    host: '192.168.1.20:8096',
  });
  expect(parseServerUrl('https://media.example.com/jellyfin')).toEqual({
    scheme: 'https',
    host: 'media.example.com/jellyfin',
  });
});

test('rejects missing host', () => {
  expect(() => buildServerUrl({ scheme: 'https', host: '' })).toThrow(
    'Server host is required',
  );
});

test('buildServerUrlEffect fails with InvalidServerUrl for missing host', () => {
  const result = Effect.runSyncExit(
    buildServerUrlEffect({ scheme: 'https', host: '' }),
  );
  expect(Exit.isFailure(result)).toBe(true);
  if (Exit.isFailure(result)) {
    const reason = result.cause.reasons[0];
    if (!reason || !Cause.isFailReason(reason)) {
      throw new Error('Expected typed InvalidServerUrl failure');
    }
    const error = reason.error;
    expect(error).toBeInstanceOf(InvalidServerUrl);
    expect(error.message).toBe('Server host is required');
  }
});

test('buildServerUrlEffect fails with InvalidServerUrl for invalid host', () => {
  const result = Effect.runSyncExit(
    buildServerUrlEffect({ scheme: 'https', host: 'not a valid host?!' }),
  );
  expect(Exit.isFailure(result)).toBe(true);
  if (Exit.isFailure(result)) {
    const reason = result.cause.reasons[0];
    if (!reason || !Cause.isFailReason(reason)) {
      throw new Error('Expected typed InvalidServerUrl failure');
    }
    const error = reason.error;
    expect(error).toBeInstanceOf(InvalidServerUrl);
    expect(error.message).toBe('Enter a valid Jellyfin server host');
  }
});
