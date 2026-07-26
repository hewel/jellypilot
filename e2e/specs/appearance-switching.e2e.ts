import { spawn } from 'node:child_process';
import { closeSync, openSync } from 'node:fs';
import { readdir, readFile, readlink, realpath } from 'node:fs/promises';
import net from 'node:net';
import path from 'node:path';

import { $, browser, expect } from '@wdio/globals';

import type {
  AppConfig,
  Appearance,
  ConnectionState,
  NowPlayingState,
  OpaqueCanvasRgb,
  SavedServiceProfiles,
  VideoHome,
} from '../../src/bindings';
import { APPEARANCE_STARTUP_CASES } from '../support/appearance-startup';

const connectedState = {
  capabilities: {
    introSkipper: true,
    quickConnect: true,
    remoteControl: true,
    remoteControlAvailable: true,
    remoteControlWarning: null,
  },
  connected: true,
  provider: 'jellyfin',
  serverName: 'Jellyfin Home',
  serverUrl: 'https://jellyfin.example.com',
  userId: 'user-1',
  userName: 'Ada',
} as const satisfies ConnectionState;

const savedProfiles = {
  activeProfileKey: 'jellyfin|https://jellyfin.example.com|Ada',
  profiles: [
    {
      active: true,
      key: 'jellyfin|https://jellyfin.example.com|Ada',
      lastRestoreError: null,
      provider: 'jellyfin',
      reauthRequired: false,
      serverName: 'Jellyfin Home',
      serverUrl: 'https://jellyfin.example.com',
      userName: 'Ada',
    },
  ],
} as const satisfies SavedServiceProfiles;

const videoHome = {
  continueWatching: [],
  nextUp: [],
  latestMovies: [],
  latestEpisodes: [],
} as const satisfies VideoHome;

const offlineState = {
  canPlayNext: false,
  canPlayPrevious: false,
  media: null,
  nextUnavailableReason: 'noCurrentItem',
  player: {
    connected: false,
    duration: 0,
    muted: false,
    paused: true,
    timePos: 0,
    volume: 100,
  },
  previousUnavailableReason: 'noCurrentItem',
  status: 'offline',
} as const satisfies NowPlayingState;

const appConfig = {
  deviceName: 'JellyPilot',
  imageDiskCacheEnabled: true,
  introSkipperMode: 'automatic',
  keybindIntroSkip: 'g',
  keybindNext: 'Shift+>',
  keybindPrev: 'Shift+<',
  mpvArgs: [],
  mpvPath: null,
  preferredSubtitleLanguages: [],
  progressInterval: 5,
  startMinimized: false,
} as const satisfies AppConfig;

const fixtures = {
  server_is_connected: true,
  server_get_state: connectedState,
  server_profiles_get: savedProfiles,
  config_get: appConfig,
  mpv_is_connected: false,
  library_video_home: videoHome,
  library_video_shortcuts: [],
  now_playing_get_state: offlineState,
} as const;

function caseByAppearance(appearance: Appearance) {
  const match = APPEARANCE_STARTUP_CASES.find(
    (entry) =>
      entry.appearance.designTheme === appearance.designTheme &&
      entry.appearance.colorMode === appearance.colorMode,
  );
  if (!match) {
    throw new Error(`Missing appearance case for ${JSON.stringify(appearance)}`);
  }
  return match;
}

async function openSettings() {
  const settings = await $('aria/Open Settings');
  await settings.waitForDisplayed({ timeout: 30_000 });
  await settings.click();
  const appearanceHeading = await $('aria/Appearance');
  await appearanceHeading.waitForDisplayed({ timeout: 30_000 });
}

async function selectRadio(name: string) {
  const radio = await $(`aria/${name}`);
  await radio.waitForDisplayed({ timeout: 30_000 });
  await radio.click();
}

async function waitForAppearance(expected: {
  readonly appearance: Appearance;
  readonly pandaTheme: string;
  readonly colorMode: string;
  readonly nativeTheme: string;
  readonly canvas: OpaqueCanvasRgb;
  readonly canvasCss: string;
}) {
  await browser.waitUntil(
    async () => {
      const snapshot = await browser.execute(
        async (expectedAppearance, expectedCanvas) => {
          const controller = window.__JELLYPILOT_E2E__;
          if (!controller) throw new Error('Missing E2E controller');
          const root = document.documentElement;
          return {
            pandaTheme: root.dataset.pandaTheme ?? null,
            colorMode: root.dataset.colorMode ?? null,
            colorScheme: root.style.colorScheme || getComputedStyle(root).colorScheme,
            bodyBackground: getComputedStyle(document.body).backgroundColor,
            appearance: await controller.invokeForTest<Appearance>('appearance_get'),
            nativeTheme: await controller.invokeForTest<string | null>('plugin:window|theme', {
              label: 'main',
            }),
            hasSetCall: controller.hasExpectedAppearanceSetCall(expectedAppearance, expectedCanvas),
          };
        },
        expected.appearance,
        expected.canvas,
      );

      return (
        snapshot.pandaTheme === expected.pandaTheme &&
        snapshot.colorMode === expected.colorMode &&
        snapshot.colorScheme.includes(expected.colorMode) &&
        snapshot.bodyBackground.replaceAll(/\s+/g, '') ===
          expected.canvasCss.replaceAll(/\s+/g, '') &&
        snapshot.appearance.designTheme === expected.appearance.designTheme &&
        snapshot.appearance.colorMode === expected.appearance.colorMode &&
        snapshot.nativeTheme === expected.nativeTheme &&
        snapshot.hasSetCall === true
      );
    },
    {
      timeout: 200,
      interval: 20,
      timeoutMsg: `Appearance did not synchronize within 200ms: ${JSON.stringify(expected.appearance)}`,
    },
  );
}

const sleep = (ms: number) => {
  const { promise, resolve } = Promise.withResolvers<void>();
  setTimeout(resolve, ms);
  return promise;
};

interface PortListener {
  readonly inode: string;
  readonly pid: number;
}

/**
 * Read-only resolution of the single process listening on `port` via /proc.
 * Returns every matching listener; the caller fails closed unless exactly one
 * is found. Never signals or otherwise touches the resolved process.
 */
async function resolveListeningPids(port: number): Promise<readonly PortListener[]> {
  const hexPort = port.toString(16).toUpperCase().padStart(4, '0');
  const inodes = new Set<string>();
  for (const table of ['/proc/net/tcp', '/proc/net/tcp6']) {
    let content: string;
    try {
      content = await readFile(table, 'utf8');
    } catch {
      continue;
    }
    for (const line of content.split('\n').slice(1)) {
      const fields = line.trim().split(/\s+/);
      if (fields.length < 10) continue;
      const localAddress = fields[1];
      const state = fields[3];
      const inode = fields[9];
      if (!localAddress || state !== '0A' || inode === '0') continue;
      const localPort = localAddress.split(':').pop();
      if (localPort === hexPort && inode) inodes.add(inode);
    }
  }
  if (inodes.size === 0) return [];

  const listeners: PortListener[] = [];
  let pids: string[];
  try {
    pids = await readdir('/proc');
  } catch {
    throw new Error(`Could not read /proc while resolving the listener on port ${port}.`);
  }
  for (const entry of pids) {
    if (!/^\d+$/.test(entry)) continue;
    const pid = Number.parseInt(entry, 10);
    let descriptors: string[];
    try {
      descriptors = await readdir(`/proc/${pid}/fd`);
    } catch {
      continue;
    }
    for (const descriptor of descriptors) {
      let target: string;
      try {
        target = await readlink(`/proc/${pid}/fd/${descriptor}`);
      } catch {
        continue;
      }
      const match = /^socket:\[(\d+)\]$/.exec(target);
      if (match?.[1] && inodes.has(match[1])) {
        listeners.push({ inode: match[1], pid });
        break;
      }
    }
  }
  return listeners;
}

/**
 * Resolve the exact current application PID listening on the sandbox's embedded
 * WebDriver port and validate through `/proc/<pid>/exe` that it is the exact
 * `JELLYPILOT_E2E_BINARY`. Fails closed on zero, multiple, unreadable, or
 * mismatched targets so termination never touches an unrelated process.
 */
async function resolveValidatedAppPid(port: number, expectedBinary: string): Promise<number> {
  const listeners = await resolveListeningPids(port);
  if (listeners.length === 0) {
    throw new Error(`No process was listening on embedded WebDriver port ${port}.`);
  }
  if (listeners.length > 1) {
    const pids = listeners.map((listener) => listener.pid).join(', ');
    throw new Error(`Multiple processes listened on port ${port}: ${pids}.`);
  }
  const pid = listeners[0]!.pid;

  let exeTarget: string;
  try {
    exeTarget = await realpath(`/proc/${pid}/exe`);
  } catch (error) {
    throw new Error(`Could not read /proc/${pid}/exe for validation.`, { cause: error });
  }
  const expected = await realpath(expectedBinary);
  if (exeTarget !== expected) {
    throw new Error(
      `Process ${pid} on port ${port} is ${exeTarget}, not the expected ${expected}.`,
    );
  }
  return pid;
}

/** Terminate one exact PID: graceful SIGTERM first, bounded SIGKILL second. */
async function terminateExactPid(pid: number): Promise<void> {
  try {
    process.kill(pid, 'SIGTERM');
  } catch {
    return; // Already exited.
  }
  const deadline = Date.now() + 5000;
  for (;;) {
    try {
      process.kill(pid, 0);
    } catch {
      return;
    }
    if (Date.now() >= deadline) break;
    await sleep(100);
  }
  try {
    process.kill(pid, 'SIGKILL');
  } catch {
    // Exited between the probe and the forced signal.
  }
}

async function portIsClosed(port: number): Promise<boolean> {
  const { promise, resolve } = Promise.withResolvers<boolean>();
  const socket = net.createConnection({ host: '127.0.0.1', port });
  socket.setTimeout(500);
  socket.once('connect', () => {
    socket.destroy();
    resolve(false);
  });
  socket.once('error', () => resolve(true));
  socket.once('timeout', () => {
    socket.destroy();
    resolve(true);
  });
  return promise;
}

/** Block until the embedded WebDriver port is closed so orchestrator teardown stays authoritative. */
async function waitForPortClosed(port: number, timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    if (await portIsClosed(port)) return;
    if (Date.now() >= deadline) {
      throw new Error(`Embedded WebDriver port ${port} remained open after cleanup.`);
    }
    await sleep(100);
  }
}

describe('appearance switching from Settings', () => {
  it('switches all four combinations, keeps design-theme native theme stable, and persists across restart', async () => {
    await browser.waitUntil(
      () => browser.execute(() => window.__JELLYPILOT_E2E__?.ready === true),
      {
        timeout: 30_000,
        timeoutMsg: 'The controlled Tauri bridge did not become ready before mount.',
      },
    );

    await browser.execute((values: typeof fixtures) => {
      const controller = window.__JELLYPILOT_E2E__;
      if (!controller?.mount) throw new Error('The E2E bridge mount was already consumed.');
      controller.installFixture('appearance_get', { kind: 'real' });
      controller.installFixture('appearance_ready', { kind: 'real' });
      controller.installFixture('appearance_set', { kind: 'real' });
      controller.installFixture('plugin:window|is_visible', { kind: 'real' });
      controller.installFixture('plugin:window|theme', { kind: 'real' });
      controller.installFixture('server_is_connected', {
        kind: 'return',
        value: values.server_is_connected,
      });
      controller.installFixture('server_get_state', {
        kind: 'return',
        value: values.server_get_state,
      });
      controller.installFixture('server_profiles_get', {
        kind: 'return',
        value: values.server_profiles_get,
      });
      controller.installFixture('config_get', {
        kind: 'return',
        value: values.config_get,
      });
      controller.installFixture('mpv_is_connected', {
        kind: 'return',
        value: values.mpv_is_connected,
      });
      controller.installFixture('library_video_home', {
        kind: 'return',
        value: values.library_video_home,
      });
      controller.installFixture('library_video_shortcuts', {
        kind: 'return',
        value: [...values.library_video_shortcuts],
      });
      controller.installFixture('now_playing_get_state', {
        kind: 'return',
        value: values.now_playing_get_state,
      });
      controller.mount();
    }, fixtures);

    await openSettings();

    const controlRoomLight = caseByAppearance({ designTheme: 'controlRoom', colorMode: 'light' });
    await selectRadio('Light');
    await waitForAppearance(controlRoomLight);

    const themeBeforeDesignSwitch = await browser.execute(async () => {
      const controller = window.__JELLYPILOT_E2E__;
      if (!controller) throw new Error('Missing E2E controller');
      return controller.invokeForTest<string | null>('plugin:window|theme', { label: 'main' });
    });
    expect(themeBeforeDesignSwitch).toBe('light');

    const braunLight = caseByAppearance({ designTheme: 'braun', colorMode: 'light' });
    await selectRadio('Braun');
    await waitForAppearance(braunLight);

    const themeAfterDesignOnly = await browser.execute(async () => {
      const controller = window.__JELLYPILOT_E2E__;
      if (!controller) throw new Error('Missing E2E controller');
      return controller.invokeForTest<string | null>('plugin:window|theme', { label: 'main' });
    });
    expect(themeAfterDesignOnly).toBe('light');

    const braunDark = caseByAppearance({ designTheme: 'braun', colorMode: 'dark' });
    await selectRadio('Dark');
    await waitForAppearance(braunDark);

    const controlRoomDark = caseByAppearance({ designTheme: 'controlRoom', colorMode: 'dark' });
    await selectRadio('Control Room');
    await waitForAppearance(controlRoomDark);

    // Final selection used for restart persistence.
    await selectRadio('Braun');
    await selectRadio('Light');
    await waitForAppearance(braunLight);

    const setPayloads = await browser.execute(() => {
      const controller = window.__JELLYPILOT_E2E__;
      if (!controller) throw new Error('Missing E2E controller');
      return controller.appearanceSetCalls();
    });
    expect(setPayloads.length).toBeGreaterThanOrEqual(4);
    expect(
      setPayloads.some((args) => {
        const request = args?.request;
        if (!request || typeof request !== 'object') return false;
        const appearance = 'appearance' in request ? request.appearance : undefined;
        const canvas = 'canvas' in request ? request.canvas : undefined;
        if (!appearance || typeof appearance !== 'object') return false;
        if (!canvas || typeof canvas !== 'object') return false;
        return (
          'designTheme' in appearance &&
          appearance.designTheme === 'braun' &&
          'colorMode' in appearance &&
          appearance.colorMode === 'light' &&
          'red' in canvas &&
          canvas.red === braunLight.canvas.red &&
          'green' in canvas &&
          canvas.green === braunLight.canvas.green &&
          'blue' in canvas &&
          canvas.blue === braunLight.canvas.blue
        );
      }),
    ).toBe(true);

    const previousSessionId = browser.sessionId;
    const appBinaryPath = process.env.JELLYPILOT_E2E_BINARY;
    const embeddedPort = browser.options.port;
    const restartLogDir = process.env.JELLYPILOT_E2E_LOG_DIR;
    if (!appBinaryPath || !embeddedPort) {
      throw new Error('Missing embedded application path/port for reloadSession restart.');
    }

    // Embedded WebDriver is the app process itself and does not expose wdio:driverPID.
    // Resolve the exact current app PID listening on the sandbox's embedded port,
    // validate through /proc/<pid>/exe that it is the expected binary, and terminate
    // only that PID. Then spawn a fresh, non-detached process on the same port so it
    // inherits the WDIO worker process group the orchestrator already tracks and
    // tears down, and open a new WebDriver session against it via reloadSession.
    // The replacement and its live session are left intact on success: WDIO's normal
    // endSession issues deleteSession, and the orchestrator's scoped process-group
    // teardown plus port verification owns every abnormal or lingering process.
    const currentAppPid = await resolveValidatedAppPid(embeddedPort, appBinaryPath);
    await terminateExactPid(currentAppPid);
    await waitForPortClosed(embeddedPort, 15_000);

    if (!restartLogDir) {
      throw new Error('Missing JELLYPILOT_E2E_LOG_DIR for the replacement application log.');
    }
    const restartLogDescriptor = openSync(path.join(restartLogDir, 'backend-restart.log'), 'a');
    const appChild = spawn(appBinaryPath, [], {
      env: {
        ...process.env,
        TAURI_WEBDRIVER_PORT: String(embeddedPort),
        WDIO_EMBEDDED_SERVER: 'true',
      },
      stdio: ['ignore', restartLogDescriptor, restartLogDescriptor],
    });
    // The child inherited the descriptor; release the parent-side copy so only the
    // replacement application holds the sandbox log open.
    closeSync(restartLogDescriptor);

    let restartVerified = false;
    appChild.on('error', (error) => {
      console.error('Replacement Tauri application process failed.', error);
    });
    appChild.on('exit', (code, signal) => {
      if (!restartVerified) {
        console.error(
          `Replacement Tauri application exited before restart verification (code=${String(code)}, signal=${String(signal)}).`,
        );
      }
    });

    const bootDeadline = Date.now() + 90_000;
    for (;;) {
      try {
        const response = await fetch(`http://127.0.0.1:${embeddedPort}/status`, {
          signal: AbortSignal.timeout(1000),
        });
        if (response.ok) break;
      } catch {
        // still booting
      }
      if (Date.now() >= bootDeadline) {
        throw new Error(
          `New Tauri application process did not expose embedded WebDriver on port ${embeddedPort}.`,
        );
      }
      await sleep(250);
    }

    await browser.reloadSession(browser.requestedCapabilities);
    expect(browser.sessionId).not.toBe(previousSessionId);

    await browser.waitUntil(
      async () => {
        try {
          const handles = await browser.getWindowHandles();
          if (handles.length === 0) return false;
          await browser.switchToWindow(handles[0]!);
          const state = await browser.execute(() => ({
            ready: window.__JELLYPILOT_E2E__?.ready === true,
            hasMount: typeof window.__JELLYPILOT_E2E__?.mount === 'function',
          }));
          return state.ready && state.hasMount;
        } catch {
          return false;
        }
      },
      {
        timeout: 60_000,
        interval: 500,
        timeoutMsg: 'The controlled Tauri bridge did not become ready after reloadSession.',
      },
    );

    await browser.execute((values: typeof fixtures) => {
      const controller = window.__JELLYPILOT_E2E__;
      if (!controller?.mount) throw new Error('The E2E bridge mount was already consumed.');
      controller.installFixture('appearance_get', { kind: 'real' });
      controller.installFixture('appearance_ready', { kind: 'real' });
      controller.installFixture('appearance_set', { kind: 'real' });
      controller.installFixture('plugin:window|is_visible', { kind: 'real' });
      controller.installFixture('plugin:window|theme', { kind: 'real' });
      controller.installFixture('server_is_connected', {
        kind: 'return',
        value: values.server_is_connected,
      });
      controller.installFixture('server_get_state', {
        kind: 'return',
        value: values.server_get_state,
      });
      controller.installFixture('server_profiles_get', {
        kind: 'return',
        value: values.server_profiles_get,
      });
      controller.installFixture('config_get', {
        kind: 'return',
        value: values.config_get,
      });
      controller.installFixture('mpv_is_connected', {
        kind: 'return',
        value: values.mpv_is_connected,
      });
      controller.installFixture('library_video_home', {
        kind: 'return',
        value: values.library_video_home,
      });
      controller.installFixture('library_video_shortcuts', {
        kind: 'return',
        value: [...values.library_video_shortcuts],
      });
      controller.installFixture('now_playing_get_state', {
        kind: 'return',
        value: values.now_playing_get_state,
      });
      controller.mount();
    }, fixtures);

    await browser.waitUntil(
      async () => {
        const snapshot = await browser.execute(async () => {
          const controller = window.__JELLYPILOT_E2E__;
          if (!controller) throw new Error('Missing E2E controller');
          const root = document.documentElement;
          return {
            pandaTheme: root.dataset.pandaTheme ?? null,
            colorMode: root.dataset.colorMode ?? null,
            appearance: await controller.invokeForTest<Appearance>('appearance_get'),
            nativeTheme: await controller.invokeForTest<string | null>('plugin:window|theme', {
              label: 'main',
            }),
            bodyBackground: getComputedStyle(document.body).backgroundColor,
          };
        });
        return (
          snapshot.pandaTheme === braunLight.pandaTheme &&
          snapshot.colorMode === braunLight.colorMode &&
          snapshot.appearance.designTheme === 'braun' &&
          snapshot.appearance.colorMode === 'light' &&
          snapshot.nativeTheme === 'light' &&
          snapshot.bodyBackground.replaceAll(/\s+/g, '') ===
            braunLight.canvasCss.replaceAll(/\s+/g, '')
        );
      },
      {
        timeout: 30_000,
        timeoutMsg: 'Persisted Braun Light appearance did not restore after session restart.',
      },
    );

    restartVerified = true;
  });
});
