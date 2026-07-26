import { afterEach, describe, expect, rstest, test } from '@rstest/core';
import { fireEvent, screen, waitFor } from '@testing-library/dom';
import { render } from 'solid-js/web';

import { commands } from '../src/bindings';
import type { Appearance, OpaqueCanvasRgb } from '../src/bindings';
import { AppearanceProvider, useAppearance } from '../src/components/AppearanceProvider';
import { ToastProvider } from '../src/components/ToastProvider';
import { CONTROL_ROOM_DARK_APPEARANCE, CONTROL_ROOM_DARK_CANVAS } from '../src/effects/appearance';

const originalGetComputedStyle = globalThis.getComputedStyle;
const originalRaf = globalThis.requestAnimationFrame;

function stubComputedBackground(color: string) {
  globalThis.getComputedStyle = (() =>
    ({
      backgroundColor: color,
      getPropertyValue: (name: string) => (name === 'background-color' ? color : ''),
    }) as CSSStyleDeclaration) as typeof globalThis.getComputedStyle;
}

function canvasCss(canvas: OpaqueCanvasRgb) {
  return `rgb(${canvas.red}, ${canvas.green}, ${canvas.blue})`;
}

function rootSnapshot() {
  return {
    pandaTheme: document.documentElement.dataset.pandaTheme,
    colorMode: document.documentElement.dataset.colorMode,
    colorScheme: document.documentElement.style.colorScheme,
  };
}

function AppearanceProbe() {
  const appearance = useAppearance();
  return (
    <div>
      <button type="button" onClick={() => appearance.selectDesignTheme('braun')}>
        Choose Braun
      </button>
      <button type="button" onClick={() => appearance.selectColorMode('light')}>
        Choose Light
      </button>
      <button
        type="button"
        onClick={() => appearance.selectAppearance({ designTheme: 'braun', colorMode: 'light' })}
      >
        Choose Braun Light
      </button>
      <button
        type="button"
        onClick={() =>
          appearance.selectAppearance({ designTheme: 'controlRoom', colorMode: 'dark' })
        }
      >
        Choose Control Room Dark
      </button>
      <output aria-label="desired-theme">{appearance.desired().designTheme}</output>
      <output aria-label="desired-mode">{appearance.desired().colorMode}</output>
      <output aria-label="confirmed-theme">{appearance.confirmed().designTheme}</output>
      <output aria-label="confirmed-mode">{appearance.confirmed().colorMode}</output>
      <output aria-label="saving">{appearance.saving() ? 'yes' : 'no'}</output>
    </div>
  );
}

function renderCoordinator(initial: Appearance = CONTROL_ROOM_DARK_APPEARANCE) {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <ToastProvider>
        <AppearanceProvider
          initial={{
            appearance: initial,
            canvas: CONTROL_ROOM_DARK_CANVAS,
          }}
        >
          <AppearanceProbe />
        </AppearanceProvider>
      </ToastProvider>
    ),
    root,
  );
  return () => {
    dispose();
    root.remove();
  };
}

afterEach(() => {
  globalThis.getComputedStyle = originalGetComputedStyle;
  globalThis.requestAnimationFrame = originalRaf;
  delete document.documentElement.dataset.pandaTheme;
  delete document.documentElement.dataset.colorMode;
  document.documentElement.style.colorScheme = '';
  rstest.restoreAllMocks();
  document.body.innerHTML = '';
});

describe('AppearanceProvider coordinator', () => {
  test('optimistically applies root attributes and confirms successful writes', async () => {
    stubComputedBackground(canvasCss({ red: 12, green: 14, blue: 18 }));
    globalThis.requestAnimationFrame = ((cb: FrameRequestCallback) => {
      cb(0);
      return 0;
    }) as typeof requestAnimationFrame;

    const appearanceSet = rstest.spyOn(commands, 'appearanceSet').mockResolvedValue({
      data: null,
      status: 'ok',
    });

    const cleanup = renderCoordinator();
    fireEvent.click(screen.getByRole('button', { name: 'Choose Braun' }));

    expect(rootSnapshot()).toEqual({
      pandaTheme: 'braun',
      colorMode: 'dark',
      colorScheme: 'dark',
    });
    expect(screen.getByLabelText('desired-theme')).toHaveTextContent('braun');
    expect(screen.getByLabelText('saving')).toHaveTextContent('yes');

    await waitFor(() => expect(appearanceSet).toHaveBeenCalledTimes(1));
    await waitFor(() =>
      expect(screen.getByLabelText('confirmed-theme')).toHaveTextContent('braun'),
    );
    expect(screen.getByLabelText('saving')).toHaveTextContent('no');
    expect(appearanceSet.mock.calls[0]?.[0]).toEqual({
      appearance: { designTheme: 'braun', colorMode: 'dark' },
      canvas: { red: 12, green: 14, blue: 18 },
    });

    cleanup();
  });

  test('coalesces rapid selections to the newest intent', async () => {
    const canvases: Record<string, string> = {
      'control-room-dark': 'rgb(5, 6, 10)',
      'control-room-light': 'rgb(246, 247, 255)',
      'braun-dark': 'rgb(12, 14, 18)',
      'braun-light': 'rgb(252, 248, 248)',
    };
    globalThis.getComputedStyle = (() => {
      const theme = document.documentElement.dataset.pandaTheme ?? 'control-room';
      const mode = document.documentElement.dataset.colorMode ?? 'dark';
      const color = canvases[`${theme}-${mode}`] ?? canvases['control-room-dark'];
      return {
        backgroundColor: color,
        getPropertyValue: (name: string) => (name === 'background-color' ? color : ''),
      } as CSSStyleDeclaration;
    }) as typeof globalThis.getComputedStyle;
    globalThis.requestAnimationFrame = ((cb: FrameRequestCallback) => {
      cb(0);
      return 0;
    }) as typeof requestAnimationFrame;

    let releaseFirst!: () => void;
    const firstGate = new Promise<void>((resolve) => {
      releaseFirst = resolve;
    });
    let call = 0;
    const appearanceSet = rstest.spyOn(commands, 'appearanceSet').mockImplementation(async () => {
      call += 1;
      if (call === 1) {
        await firstGate;
      }
      return { data: null, status: 'ok' };
    });

    const cleanup = renderCoordinator();
    fireEvent.click(screen.getByRole('button', { name: 'Choose Braun' }));
    fireEvent.click(screen.getByRole('button', { name: 'Choose Light' }));
    fireEvent.click(screen.getByRole('button', { name: 'Choose Braun Light' }));

    expect(rootSnapshot()).toEqual({
      pandaTheme: 'braun',
      colorMode: 'light',
      colorScheme: 'light',
    });
    expect(screen.getByLabelText('desired-theme')).toHaveTextContent('braun');
    expect(screen.getByLabelText('desired-mode')).toHaveTextContent('light');

    releaseFirst();

    await waitFor(() => expect(appearanceSet).toHaveBeenCalled());
    await waitFor(() =>
      expect(screen.getByLabelText('confirmed-theme')).toHaveTextContent('braun'),
    );
    await waitFor(() => expect(screen.getByLabelText('confirmed-mode')).toHaveTextContent('light'));

    const payloads = appearanceSet.mock.calls.map((entry) => entry[0]?.appearance);
    expect(payloads.at(-1)).toEqual({ designTheme: 'braun', colorMode: 'light' });
    expect(rootSnapshot()).toEqual({
      pandaTheme: 'braun',
      colorMode: 'light',
      colorScheme: 'light',
    });

    cleanup();
  });

  test('obsolete failure does not roll back a newer pending selection', async () => {
    const canvases: Record<string, string> = {
      'control-room-dark': 'rgb(5, 6, 10)',
      'braun-dark': 'rgb(12, 14, 18)',
      'braun-light': 'rgb(252, 248, 248)',
    };
    globalThis.getComputedStyle = (() => {
      const theme = document.documentElement.dataset.pandaTheme ?? 'control-room';
      const mode = document.documentElement.dataset.colorMode ?? 'dark';
      const color = canvases[`${theme}-${mode}`] ?? canvases['control-room-dark'];
      return {
        backgroundColor: color,
        getPropertyValue: (name: string) => (name === 'background-color' ? color : ''),
      } as CSSStyleDeclaration;
    }) as typeof globalThis.getComputedStyle;
    globalThis.requestAnimationFrame = ((cb: FrameRequestCallback) => {
      cb(0);
      return 0;
    }) as typeof requestAnimationFrame;

    let releaseFirst!: (value: {
      status: 'error';
      error: { code: 'internal'; message: string };
    }) => void;
    const firstGate = new Promise<{
      status: 'error';
      error: { code: 'internal'; message: string };
    }>((resolve) => {
      releaseFirst = resolve;
    });
    let call = 0;
    const appearanceSet = rstest.spyOn(commands, 'appearanceSet').mockImplementation(async () => {
      call += 1;
      if (call === 1) {
        await firstGate;
        return {
          status: 'error',
          error: { code: 'internal', message: 'stale write failed' },
        };
      }
      return { data: null, status: 'ok' };
    });

    const cleanup = renderCoordinator();
    fireEvent.click(screen.getByRole('button', { name: 'Choose Braun' }));

    // Wait until the first write has passed paint and entered IPC, then select newer.
    await waitFor(() => expect(appearanceSet).toHaveBeenCalledTimes(1));
    fireEvent.click(screen.getByRole('button', { name: 'Choose Braun Light' }));

    expect(rootSnapshot()).toEqual({
      pandaTheme: 'braun',
      colorMode: 'light',
      colorScheme: 'light',
    });

    releaseFirst({
      status: 'error',
      error: { code: 'internal', message: 'stale write failed' },
    });

    await waitFor(() => expect(appearanceSet.mock.calls.length).toBeGreaterThanOrEqual(2));
    await waitFor(() => expect(screen.getByLabelText('confirmed-mode')).toHaveTextContent('light'));
    expect(screen.queryByRole('alert')).toBeNull();
    expect(rootSnapshot()).toEqual({
      pandaTheme: 'braun',
      colorMode: 'light',
      colorScheme: 'light',
    });
    expect(
      appearanceSet.mock.calls.some((entry) => entry[0]?.appearance.colorMode === 'light'),
    ).toBe(true);

    cleanup();
  });

  test('newest failure restores confirmed appearance, toasts once, and remains retryable', async () => {
    stubComputedBackground(canvasCss({ red: 12, green: 14, blue: 18 }));
    globalThis.requestAnimationFrame = ((cb: FrameRequestCallback) => {
      cb(0);
      return 0;
    }) as typeof requestAnimationFrame;

    const appearanceSet = rstest
      .spyOn(commands, 'appearanceSet')
      .mockResolvedValueOnce({
        status: 'error',
        error: { code: 'internal', message: 'persist failed' },
      })
      .mockResolvedValueOnce({
        data: null,
        status: 'ok',
      });

    const cleanup = renderCoordinator();
    fireEvent.click(screen.getByRole('button', { name: 'Choose Braun' }));

    await waitFor(() => expect(appearanceSet).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('persist failed'));
    expect(rootSnapshot()).toEqual({
      pandaTheme: 'control-room',
      colorMode: 'dark',
      colorScheme: 'dark',
    });
    expect(screen.getByLabelText('desired-theme')).toHaveTextContent('controlRoom');
    expect(screen.getByLabelText('confirmed-theme')).toHaveTextContent('controlRoom');
    expect(screen.getAllByRole('alert')).toHaveLength(1);

    fireEvent.click(screen.getByRole('button', { name: 'Choose Braun' }));
    await waitFor(() => expect(appearanceSet).toHaveBeenCalledTimes(2));
    await waitFor(() =>
      expect(screen.getByLabelText('confirmed-theme')).toHaveTextContent('braun'),
    );
    expect(rootSnapshot()).toEqual({
      pandaTheme: 'braun',
      colorMode: 'dark',
      colorScheme: 'dark',
    });

    cleanup();
  });

  test('paint wait cannot mix older Appearance with newer canvas', async () => {
    const canvases: Record<string, string> = {
      'control-room-dark': 'rgb(5, 6, 10)',
      'braun-dark': 'rgb(12, 14, 18)',
      'braun-light': 'rgb(252, 248, 248)',
    };
    globalThis.getComputedStyle = (() => {
      const theme = document.documentElement.dataset.pandaTheme ?? 'control-room';
      const mode = document.documentElement.dataset.colorMode ?? 'dark';
      const color = canvases[`${theme}-${mode}`] ?? canvases['control-room-dark'];
      return {
        backgroundColor: color,
        getPropertyValue: (name: string) => (name === 'background-color' ? color : ''),
      } as CSSStyleDeclaration;
    }) as typeof globalThis.getComputedStyle;

    let firstPaintCallback: FrameRequestCallback | null = null;
    let paintScheduled = false;
    const releasePaint = () => {
      firstPaintCallback?.(0);
      firstPaintCallback = null;
    };
    globalThis.requestAnimationFrame = ((cb: FrameRequestCallback) => {
      if (!paintScheduled) {
        paintScheduled = true;
        firstPaintCallback = cb;
        return 1;
      }
      cb(0);
      return 2;
    }) as typeof requestAnimationFrame;

    const appearanceSet = rstest.spyOn(commands, 'appearanceSet').mockResolvedValue({
      data: null,
      status: 'ok',
    });

    const cleanup = renderCoordinator();
    fireEvent.click(screen.getByRole('button', { name: 'Choose Braun' }));

    await waitFor(() => expect(paintScheduled).toBe(true));
    expect(rootSnapshot()).toEqual({
      pandaTheme: 'braun',
      colorMode: 'dark',
      colorScheme: 'dark',
    });

    // Newer selection arrives after older root was applied but before its paint resolves.
    fireEvent.click(screen.getByRole('button', { name: 'Choose Braun Light' }));
    expect(rootSnapshot()).toEqual({
      pandaTheme: 'braun',
      colorMode: 'light',
      colorScheme: 'light',
    });

    releasePaint();

    await waitFor(() => expect(appearanceSet).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByLabelText('confirmed-mode')).toHaveTextContent('light'));

    const payloads = appearanceSet.mock.calls.map((entry) => entry[0]);
    expect(
      payloads.some(
        (payload) =>
          payload?.appearance.designTheme === 'braun' &&
          payload.appearance.colorMode === 'dark' &&
          payload.canvas.red === 252,
      ),
    ).toBe(false);
    expect(payloads.at(-1)).toEqual({
      appearance: { designTheme: 'braun', colorMode: 'light' },
      canvas: { red: 252, green: 248, blue: 248 },
    });
    expect(rootSnapshot()).toEqual({
      pandaTheme: 'braun',
      colorMode: 'light',
      colorScheme: 'light',
    });

    cleanup();
  });

  test('older success advances confirmed so a failed newest write rolls back to it', async () => {
    const canvases: Record<string, string> = {
      'control-room-dark': 'rgb(5, 6, 10)',
      'braun-dark': 'rgb(12, 14, 18)',
      'braun-light': 'rgb(252, 248, 248)',
    };
    globalThis.getComputedStyle = (() => {
      const theme = document.documentElement.dataset.pandaTheme ?? 'control-room';
      const mode = document.documentElement.dataset.colorMode ?? 'dark';
      const color = canvases[`${theme}-${mode}`] ?? canvases['control-room-dark'];
      return {
        backgroundColor: color,
        getPropertyValue: (name: string) => (name === 'background-color' ? color : ''),
      } as CSSStyleDeclaration;
    }) as typeof globalThis.getComputedStyle;
    globalThis.requestAnimationFrame = ((cb: FrameRequestCallback) => {
      cb(0);
      return 0;
    }) as typeof requestAnimationFrame;

    let releaseFirst!: (value: { data: null; status: 'ok' }) => void;
    const firstGate = new Promise<{ data: null; status: 'ok' }>((resolve) => {
      releaseFirst = resolve;
    });
    let call = 0;
    const appearanceSet = rstest.spyOn(commands, 'appearanceSet').mockImplementation(async () => {
      call += 1;
      if (call === 1) {
        await firstGate;
        return { data: null, status: 'ok' };
      }
      return {
        status: 'error',
        error: { code: 'internal', message: 'newest write failed' },
      };
    });

    const cleanup = renderCoordinator();
    fireEvent.click(screen.getByRole('button', { name: 'Choose Braun' }));

    // Wait until the older write has entered IPC, then queue a newer intent.
    await waitFor(() => expect(appearanceSet).toHaveBeenCalledTimes(1));
    fireEvent.click(screen.getByRole('button', { name: 'Choose Braun Light' }));

    expect(rootSnapshot()).toEqual({
      pandaTheme: 'braun',
      colorMode: 'light',
      colorScheme: 'light',
    });

    // Older write succeeds while the newer intent is queued.
    releaseFirst({ data: null, status: 'ok' });

    await waitFor(() => expect(appearanceSet).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('newest write failed'));

    // Rollback stops at the older successful Appearance, not the original root.
    expect(rootSnapshot()).toEqual({
      pandaTheme: 'braun',
      colorMode: 'dark',
      colorScheme: 'dark',
    });
    expect(screen.getByLabelText('desired-mode')).toHaveTextContent('dark');
    expect(screen.getByLabelText('confirmed-theme')).toHaveTextContent('braun');
    expect(screen.getByLabelText('confirmed-mode')).toHaveTextContent('dark');
    expect(screen.getAllByRole('alert')).toHaveLength(1);

    const payloads = appearanceSet.mock.calls.map((entry) => entry[0]?.appearance);
    expect(payloads).toEqual([
      { designTheme: 'braun', colorMode: 'dark' },
      { designTheme: 'braun', colorMode: 'light' },
    ]);

    cleanup();
  });

  test('queued return to the prior confirmed Appearance still sends a compensating write', async () => {
    const canvases: Record<string, string> = {
      'control-room-dark': 'rgb(5, 6, 10)',
      'braun-dark': 'rgb(12, 14, 18)',
    };
    globalThis.getComputedStyle = (() => {
      const theme = document.documentElement.dataset.pandaTheme ?? 'control-room';
      const mode = document.documentElement.dataset.colorMode ?? 'dark';
      const color = canvases[`${theme}-${mode}`] ?? canvases['control-room-dark'];
      return {
        backgroundColor: color,
        getPropertyValue: (name: string) => (name === 'background-color' ? color : ''),
      } as CSSStyleDeclaration;
    }) as typeof globalThis.getComputedStyle;
    globalThis.requestAnimationFrame = ((cb: FrameRequestCallback) => {
      cb(0);
      return 0;
    }) as typeof requestAnimationFrame;

    let releaseFirst!: (value: { data: null; status: 'ok' }) => void;
    const firstGate = new Promise<{ data: null; status: 'ok' }>((resolve) => {
      releaseFirst = resolve;
    });
    let call = 0;
    const appearanceSet = rstest.spyOn(commands, 'appearanceSet').mockImplementation(async () => {
      call += 1;
      if (call === 1) {
        await firstGate;
      }
      return { data: null, status: 'ok' };
    });

    const cleanup = renderCoordinator();
    fireEvent.click(screen.getByRole('button', { name: 'Choose Braun' }));

    await waitFor(() => expect(appearanceSet).toHaveBeenCalledTimes(1));
    // Return to the pre-write confirmed Appearance while the older write is in flight.
    fireEvent.click(screen.getByRole('button', { name: 'Choose Control Room Dark' }));

    expect(rootSnapshot()).toEqual({
      pandaTheme: 'control-room',
      colorMode: 'dark',
      colorScheme: 'dark',
    });

    releaseFirst({ data: null, status: 'ok' });

    await waitFor(() => expect(appearanceSet).toHaveBeenCalledTimes(2));
    await waitFor(() =>
      expect(screen.getByLabelText('confirmed-theme')).toHaveTextContent('controlRoom'),
    );
    expect(screen.getByLabelText('confirmed-mode')).toHaveTextContent('dark');
    expect(screen.getByLabelText('saving')).toHaveTextContent('no');
    expect(screen.queryByRole('alert')).toBeNull();

    const payloads = appearanceSet.mock.calls.map((entry) => entry[0]?.appearance);
    expect(payloads).toEqual([
      { designTheme: 'braun', colorMode: 'dark' },
      { designTheme: 'controlRoom', colorMode: 'dark' },
    ]);

    cleanup();
  });
});
