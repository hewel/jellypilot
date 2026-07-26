import { afterEach, describe, expect, rstest, test } from '@rstest/core';
import { Effect, Exit } from 'effect';

import { commands } from '../src/bindings';
import type { Appearance, OpaqueCanvasRgb } from '../src/bindings';
import {
  applyAppearanceRootAttributes,
  bootstrapAppearance,
  CONTROL_ROOM_DARK_APPEARANCE,
  CONTROL_ROOM_DARK_CANVAS,
  parseOpaqueCssColor,
} from '../src/effects/appearance';
import { AppearanceCanvasError, CommandError } from '../src/effects/errors';

const originalGetComputedStyle = globalThis.getComputedStyle;

afterEach(() => {
  delete document.documentElement.dataset.pandaTheme;
  delete document.documentElement.dataset.colorMode;
  document.documentElement.style.colorScheme = '';
  globalThis.getComputedStyle = originalGetComputedStyle;
  rstest.restoreAllMocks();
});

function stubComputedBackground(color: string) {
  globalThis.getComputedStyle = ((element: Element) => {
    if (element === document.documentElement || element === document.body) {
      return { backgroundColor: color } as CSSStyleDeclaration;
    }
    return originalGetComputedStyle(element);
  }) as typeof globalThis.getComputedStyle;
}

describe('appearance bootstrap helpers', () => {
  test('applies panda theme, color mode, and CSS color-scheme root attributes', () => {
    const root = document.documentElement;
    const applied = applyAppearanceRootAttributes(root, {
      designTheme: 'braun',
      colorMode: 'light',
    });

    expect(applied).toEqual({ pandaTheme: 'braun', colorMode: 'light' });
    expect(root.dataset.pandaTheme).toBe('braun');
    expect(root.dataset.colorMode).toBe('light');
    expect(root.style.colorScheme).toBe('light');
  });

  test('parses only opaque CSS colors into typed RGB payloads', () => {
    expect(parseOpaqueCssColor('#05060a')).toEqual({ red: 5, green: 6, blue: 10 });
    expect(parseOpaqueCssColor('rgb(5, 6, 10)')).toEqual({ red: 5, green: 6, blue: 10 });
    expect(parseOpaqueCssColor('rgba(5, 6, 10, 1)')).toEqual({ red: 5, green: 6, blue: 10 });
    expect(parseOpaqueCssColor('rgba(5, 6, 10, 0.5)')).toBeNull();
    expect(parseOpaqueCssColor('transparent')).toBeNull();
  });

  test('hydrates all four appearance attribute combinations', () => {
    const combinations: {
      appearance: Appearance;
      pandaTheme: string;
      colorMode: string;
      canvas: OpaqueCanvasRgb;
      css: string;
    }[] = [
      {
        appearance: { designTheme: 'controlRoom', colorMode: 'dark' },
        pandaTheme: 'control-room',
        colorMode: 'dark',
        canvas: { red: 5, green: 6, blue: 10 },
        css: 'rgb(5, 6, 10)',
      },
      {
        appearance: { designTheme: 'controlRoom', colorMode: 'light' },
        pandaTheme: 'control-room',
        colorMode: 'light',
        canvas: { red: 246, green: 247, blue: 255 },
        css: 'rgb(246, 247, 255)',
      },
      {
        appearance: { designTheme: 'braun', colorMode: 'light' },
        pandaTheme: 'braun',
        colorMode: 'light',
        canvas: { red: 252, green: 248, blue: 248 },
        css: 'rgb(252, 248, 248)',
      },
      {
        appearance: { designTheme: 'braun', colorMode: 'dark' },
        pandaTheme: 'braun',
        colorMode: 'dark',
        canvas: { red: 12, green: 14, blue: 18 },
        css: 'rgb(12, 14, 18)',
      },
    ];

    for (const combination of combinations) {
      applyAppearanceRootAttributes(document.documentElement, combination.appearance);
      expect(document.documentElement.dataset.pandaTheme).toBe(combination.pandaTheme);
      expect(document.documentElement.dataset.colorMode).toBe(combination.colorMode);
      expect(document.documentElement.style.colorScheme).toBe(combination.colorMode);
      expect(parseOpaqueCssColor(combination.css)).toEqual(combination.canvas);
    }
  });
});

describe('bootstrapAppearance', () => {
  test('keeps appearance fetch failures as CommandError', async () => {
    stubComputedBackground('rgb(5, 6, 10)');
    rstest.spyOn(commands, 'appearanceGet').mockRejectedValue(new Error('IPC unavailable'));

    const flipped = await Effect.runPromiseExit(bootstrapAppearance.pipe(Effect.flip));
    expect(Exit.isSuccess(flipped)).toBe(true);
    if (!Exit.isSuccess(flipped)) return;
    expect(flipped.value).toBeInstanceOf(CommandError);
    expect(document.documentElement.dataset.pandaTheme).toBeUndefined();
  });

  test('applies fetched appearance and computed canvas before mount', async () => {
    stubComputedBackground('rgb(252, 248, 248)');
    rstest.spyOn(commands, 'appearanceGet').mockResolvedValue({
      designTheme: 'braun',
      colorMode: 'light',
    });

    const result = await Effect.runPromise(bootstrapAppearance);
    expect(result.appearance).toEqual({ designTheme: 'braun', colorMode: 'light' });
    expect(result.canvas).toEqual({ red: 252, green: 248, blue: 248 });
    expect(document.documentElement.dataset.pandaTheme).toBe('braun');
    expect(document.documentElement.dataset.colorMode).toBe('light');
    expect(document.documentElement.style.colorScheme).toBe('light');
  });

  test('fails with AppearanceCanvasError when computed color is translucent', async () => {
    stubComputedBackground('rgba(12, 14, 18, 0.4)');
    rstest.spyOn(commands, 'appearanceGet').mockResolvedValue({
      designTheme: 'braun',
      colorMode: 'dark',
    });

    const flipped = await Effect.runPromiseExit(bootstrapAppearance.pipe(Effect.flip));
    expect(Exit.isSuccess(flipped)).toBe(true);
    if (!Exit.isSuccess(flipped)) return;
    expect(flipped.value).toBeInstanceOf(AppearanceCanvasError);
    expect(document.documentElement.dataset.pandaTheme).toBe('braun');
    expect(document.documentElement.dataset.colorMode).toBe('dark');
  });

  test('never fabricates Control Room Dark success from opaque canvas failure alone', async () => {
    stubComputedBackground('transparent');
    rstest.spyOn(commands, 'appearanceGet').mockResolvedValue({
      designTheme: 'controlRoom',
      colorMode: 'light',
    });

    await expect(Effect.runPromise(bootstrapAppearance)).rejects.toBeInstanceOf(
      AppearanceCanvasError,
    );
    expect(CONTROL_ROOM_DARK_APPEARANCE.designTheme).toBe('controlRoom');
    expect(CONTROL_ROOM_DARK_CANVAS).toEqual({ red: 5, green: 6, blue: 10 });
  });
});
