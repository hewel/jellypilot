import { expect, test } from 'bun:test';
import { mkdtemp, mkdir, rm } from 'node:fs/promises';
import { createConnection, createServer } from 'node:net';
import { tmpdir } from 'node:os';
import path from 'node:path';

import { audioSessionEnvironment } from './native-regression';

test('audio endpoints remain reachable after app runtime isolation', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'jellypilot-audio-test-'));
  const session = path.join(root, 'session');
  const isolated = path.join(root, 'private');
  await mkdir(path.join(session, 'pulse'), { recursive: true });
  await mkdir(isolated);
  const sockets = [path.join(session, 'pipewire-0'), path.join(session, 'pulse/native')];
  const servers = sockets.map(() => createServer((socket) => socket.end()));
  try {
    await Promise.all(
      servers.map(
        (server, index) =>
          new Promise<void>((resolve, reject) => {
            server.once('error', reject);
            server.listen(sockets[index], resolve);
          }),
      ),
    );
    const environment: Readonly<Record<string, string>> = {
      XDG_RUNTIME_DIR: isolated,
      ...audioSessionEnvironment({ XDG_RUNTIME_DIR: session }),
    };
    const resolved = [
      path.join(environment.PIPEWIRE_RUNTIME_DIR ?? environment.XDG_RUNTIME_DIR, 'pipewire-0'),
      path.join(
        environment.PULSE_RUNTIME_PATH ?? path.join(environment.XDG_RUNTIME_DIR, 'pulse'),
        'native',
      ),
    ];
    await Promise.all(
      resolved.map(
        (socketPath) =>
          new Promise<void>((resolve, reject) => {
            const socket = createConnection(socketPath);
            socket.once('error', reject);
            socket.once('connect', () => {
              socket.destroy();
              resolve();
            });
          }),
      ),
    );
    expect(environment.XDG_RUNTIME_DIR).toBe(isolated);
  } finally {
    await Promise.all(
      servers.map((server) => new Promise<void>((resolve) => server.close(() => resolve()))),
    );
    await rm(root, { recursive: true, force: true });
  }
});

test('explicit audio routing survives instead of being replaced by default session paths', () => {
  const explicit = {
    PIPEWIRE_RUNTIME_DIR: '/session/custom-pipewire',
    PIPEWIRE_REMOTE: 'studio',
    PULSE_RUNTIME_PATH: '/session/custom-pulse',
    PULSE_SERVER: 'unix:/session/custom-pulse/native',
  };
  expect(audioSessionEnvironment({ XDG_RUNTIME_DIR: '/other-session', ...explicit })).toEqual(
    explicit,
  );
  expect(audioSessionEnvironment({})).toEqual({});
});
