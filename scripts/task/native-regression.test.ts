import { expect, test } from 'bun:test';
import { mkdtemp, mkdir, rm } from 'node:fs/promises';
import { createConnection, createServer } from 'node:net';
import { tmpdir } from 'node:os';
import path from 'node:path';

import { audioSessionEnvironment, nativeReport } from './native-regression';

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

const runId = '9389f864-d104-424a-aef9-fc3098201923';
const timelineReport = {
  schemaVersion: 1,
  runId,
  scenario: 'gpu',
  status: 'pass',
  requestedHostAbi: '3',
  actualHostAbi: 3,
  checks: ['stop acknowledged', 'retained compositor cleanup completed'],
  decoderSamples: [{ 'hwdec-current': 'vaapi', 'current-ao': 'pipewire' }],
};

test('timeline request cannot claim pass after legacy fallback or without host evidence', () => {
  for (const actualHostAbi of [1, 2, null]) {
    const decoded = nativeReport({ ...timelineReport, actualHostAbi }, 'gpu', runId, '3');
    expect(decoded).toMatchObject({
      status: 'fail',
      requestedHostAbi: '3',
      actualHostAbi,
    });
  }
});

test('legacy requires actual v1 or v2 rather than accidentally exercising timeline v3', () => {
  for (const actualHostAbi of [1, 2, 3]) {
    const decoded = nativeReport(
      { ...timelineReport, requestedHostAbi: 'legacy', actualHostAbi },
      'gpu',
      runId,
      'legacy',
    );
    expect(decoded.status).toBe(actualHostAbi === 3 ? 'fail' : 'pass');
  }
});

test('a native report for another requested ABI cannot satisfy the current run', () => {
  const decoded = nativeReport(
    { ...timelineReport, requestedHostAbi: 'legacy' },
    'gpu',
    runId,
    '3',
  );
  expect(decoded.status).toBe('fail');
});

test('startup unavailability with no host remains unavailable rather than manufacturing ABI evidence', () => {
  const report = {
    ...timelineReport,
    status: 'unavailable',
    actualHostAbi: null,
    error: 'No native hardware Vulkan adapter',
  };
  expect(nativeReport(report, 'gpu', runId, '3')).toMatchObject({
    status: 'unavailable',
    actualHostAbi: null,
    error: report.error,
    native: report,
  });
});

test('report decoding rejects stale identities, malformed ABI values and pre-ABI reports', () => {
  const { actualHostAbi: _actual, ...withoutActual } = timelineReport;
  const { requestedHostAbi: _requested, ...withoutRequested } = timelineReport;
  for (const report of [
    { ...timelineReport, runId: 'older-run' },
    { ...timelineReport, scenario: 'tray' },
    { ...timelineReport, actualHostAbi: '3' },
    { ...timelineReport, actualHostAbi: 2.5 },
    withoutActual,
    withoutRequested,
  ]) {
    expect(() => nativeReport(report, 'gpu', runId, '3')).toThrow();
  }
});

const sustainedReport = {
  ...timelineReport,
  sustained: { requestedSeconds: 60, elapsedSeconds: 62.5, completedCycles: 2 },
};

test('sustained success requires elapsed minimum and a completed cycle in the current run', () => {
  expect(nativeReport(sustainedReport, 'gpu', runId, '3', 60).status).toBe('pass');
  expect(
    nativeReport(
      {
        ...sustainedReport,
        sustained: { requestedSeconds: 60, elapsedSeconds: 60, completedCycles: 1 },
      },
      'gpu',
      runId,
      '3',
      60,
    ).status,
  ).toBe('pass');
  expect(() =>
    nativeReport({ ...sustainedReport, runId: 'previous-run' }, 'gpu', runId, '3', 60),
  ).toThrow();
});

test('a short-run pass or another duration cannot satisfy a sustained request', () => {
  for (const report of [
    timelineReport,
    { ...timelineReport, sustained: null },
    {
      ...sustainedReport,
      sustained: { requestedSeconds: 30, elapsedSeconds: 62, completedCycles: 2 },
    },
    {
      ...sustainedReport,
      sustained: { requestedSeconds: '60', elapsedSeconds: 62, completedCycles: 2 },
    },
  ]) {
    expect(nativeReport(report, 'gpu', runId, '3', 60).status).toBe('fail');
  }
});

test('sustained pass cannot omit timing evidence, finish early or invent completed cycles', () => {
  for (const sustained of [
    {},
    60,
    { requestedSeconds: 60, completedCycles: 1 },
    { requestedSeconds: 60, elapsedSeconds: 60 },
    ...[59.999, -1, Number.NaN, Infinity, -Infinity, '60'].map((elapsedSeconds) => ({
      requestedSeconds: 60,
      elapsedSeconds,
      completedCycles: 1,
    })),
    ...[0, -1, 0.5, Number.NaN, Infinity, '1'].map((completedCycles) => ({
      requestedSeconds: 60,
      elapsedSeconds: 60,
      completedCycles,
    })),
  ]) {
    expect(nativeReport({ ...timelineReport, sustained }, 'gpu', runId, '3', 60).status).toBe(
      'fail',
    );
  }
});

test('default short runs reject sustained pass evidence instead of reusing another mode', () => {
  expect(nativeReport(sustainedReport, 'gpu', runId, '3').status).toBe('fail');
  expect(nativeReport({ ...timelineReport, sustained: null }, 'gpu', runId, '3').status).toBe(
    'pass',
  );
});

test('sustained timing validation cannot conceal genuine failure or missing prerequisites', () => {
  for (const status of ['fail', 'unavailable']) {
    const report = {
      ...timelineReport,
      status,
      actualHostAbi: null,
      sustained: { requestedSeconds: 60, elapsedSeconds: 12, completedCycles: 0 },
      error:
        status === 'fail' ? 'Playback failed during seek' : 'No native hardware Vulkan adapter',
    };
    expect(nativeReport(report, 'gpu', runId, '3', 60)).toMatchObject({
      status,
      error: report.error,
      native: report,
    });
    expect(
      nativeReport({ ...report, sustained: sustainedReport.sustained }, 'gpu', runId, '3', 60),
    ).toMatchObject({ status, error: report.error });
  }
});
