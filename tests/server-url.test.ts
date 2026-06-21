import { expect, test } from '@rstest/core';
import { Cause, Effect, Exit } from 'effect';

import { InvalidServerUrl } from '../src/effects/errors';
import {
  buildServerUrl,
  buildServerUrlEffect,
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
  expect(buildServerUrl({ host: '192.168.1.20', scheme: 'http' }).url).toBe(
    'http://192.168.1.20:8096',
  );
  expect(buildServerUrl({ host: 'media.example.com', scheme: 'https' }).url).toBe(
    'https://media.example.com',
  );
});

test('preserves explicit ports and path prefixes', () => {
  expect(buildServerUrl({ host: 'media.example.com:443/jellyfin', scheme: 'https' }).url).toBe(
    'https://media.example.com:443/jellyfin',
  );
  expect(buildServerUrl({ host: 'jellyfin.local:8097/base', scheme: 'http' }).url).toBe(
    'http://jellyfin.local:8097/base',
  );
  expect(
    buildServerUrl({
      host: 'http://media.example.com:8080/base',
      scheme: 'https',
    }).url,
  ).toBe('https://media.example.com:8080/base');
});

test('buildServerUrlEffect preserves normalized server url outputs', () => {
  const local = Effect.runSync(buildServerUrlEffect({ host: '192.168.1.20', scheme: 'http' }));
  expect(local).toEqual({
    isLocal: true,
    url: 'http://192.168.1.20:8096',
  });

  const reverseProxy = Effect.runSync(
    buildServerUrlEffect({
      host: 'media.example.com:443/jellyfin',
      scheme: 'https',
    }),
  );
  expect(reverseProxy).toEqual({
    isLocal: false,
    url: 'https://media.example.com:443/jellyfin',
  });
});

test('parses saved final urls into visible fields', () => {
  expect(parseServerUrl('http://192.168.1.20:8096')).toEqual({
    host: '192.168.1.20:8096',
    scheme: 'http',
  });
  expect(parseServerUrl('https://media.example.com/jellyfin')).toEqual({
    host: 'media.example.com/jellyfin',
    scheme: 'https',
  });
});

test('rejects missing host', () => {
  expect(() => buildServerUrl({ host: '', scheme: 'https' })).toThrow('Server host is required');
});

test('buildServerUrlEffect fails with InvalidServerUrl for missing host', () => {
  const result = Effect.runSyncExit(buildServerUrlEffect({ host: '', scheme: 'https' }));
  expect(Exit.isFailure(result)).toBe(true);
  if (Exit.isFailure(result)) {
    const reason = result.cause.reasons[0];
    if (!reason || !Cause.isFailReason(reason)) {
      throw new Error('Expected typed InvalidServerUrl failure');
    }
    const { error } = reason;
    expect(error).toBeInstanceOf(InvalidServerUrl);
    expect(error.message).toBe('Server host is required');
  }
});

test('buildServerUrlEffect fails with InvalidServerUrl for invalid host', () => {
  const result = Effect.runSyncExit(
    buildServerUrlEffect({ host: 'not a valid host?!', scheme: 'https' }),
  );
  expect(Exit.isFailure(result)).toBe(true);
  if (Exit.isFailure(result)) {
    const reason = result.cause.reasons[0];
    if (!reason || !Cause.isFailReason(reason)) {
      throw new Error('Expected typed InvalidServerUrl failure');
    }
    const { error } = reason;
    expect(error).toBeInstanceOf(InvalidServerUrl);
    expect(error.message).toBe('Enter a valid Jellyfin server host');
  }
});
