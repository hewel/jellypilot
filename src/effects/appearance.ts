import { commands } from '@bindings';
import type { Appearance, AppearanceSetRequest, OpaqueCanvasRgb } from '@bindings';
import { Effect } from 'effect';

import { runTauriCommand, runTauriCommandRaw } from './commands';
import { AppearanceCanvasError, type CommandError } from './errors';

export const CONTROL_ROOM_DARK_APPEARANCE = {
  designTheme: 'controlRoom',
  colorMode: 'dark',
} as const satisfies Appearance;

export const CONTROL_ROOM_DARK_CANVAS = {
  red: 5,
  green: 6,
  blue: 10,
} as const satisfies OpaqueCanvasRgb;

export type PandaThemeAttribute = 'control-room' | 'braun';
export type ColorModeAttribute = 'light' | 'dark';

export interface BootstrappedAppearance {
  readonly appearance: Appearance;
  readonly canvas: OpaqueCanvasRgb;
}

export function applyAppearanceRootAttributes(
  root: HTMLElement,
  appearance: Appearance,
): {
  readonly pandaTheme: PandaThemeAttribute;
  readonly colorMode: ColorModeAttribute;
} {
  const pandaTheme: PandaThemeAttribute =
    appearance.designTheme === 'braun' ? 'braun' : 'control-room';
  const colorMode: ColorModeAttribute = appearance.colorMode === 'light' ? 'light' : 'dark';

  root.dataset.pandaTheme = pandaTheme;
  root.dataset.colorMode = colorMode;
  root.style.colorScheme = colorMode;

  return { pandaTheme, colorMode };
}

const CHANNEL_MAX = 255;

function parseChannel(value: string): number | null {
  const trimmed = value.trim();
  if (trimmed.length === 0) return null;
  if (trimmed.endsWith('%')) {
    const percent = Number.parseFloat(trimmed.slice(0, -1));
    if (!Number.isFinite(percent)) return null;
    return Math.round((percent / 100) * CHANNEL_MAX);
  }
  const absolute = Number.parseFloat(trimmed);
  if (!Number.isFinite(absolute)) return null;
  return Math.round(absolute);
}

/** Strictly convert a computed CSS color into an opaque RGB payload. */
export function parseOpaqueCssColor(color: string): OpaqueCanvasRgb | null {
  const value = color.trim().toLowerCase();
  if (value.length === 0 || value === 'transparent') return null;

  if (value.startsWith('#')) {
    const hex = value.slice(1);
    if (hex.length === 3 || hex.length === 4) {
      const red = Number.parseInt(hex[0] + hex[0], 16);
      const green = Number.parseInt(hex[1] + hex[1], 16);
      const blue = Number.parseInt(hex[2] + hex[2], 16);
      if ([red, green, blue].some((channel) => Number.isNaN(channel))) return null;
      if (hex.length === 4) {
        const alpha = Number.parseInt(hex[3] + hex[3], 16) / CHANNEL_MAX;
        if (alpha < 1) return null;
      }
      return { red, green, blue };
    }
    if (hex.length === 6 || hex.length === 8) {
      const red = Number.parseInt(hex.slice(0, 2), 16);
      const green = Number.parseInt(hex.slice(2, 4), 16);
      const blue = Number.parseInt(hex.slice(4, 6), 16);
      if ([red, green, blue].some((channel) => Number.isNaN(channel))) return null;
      if (hex.length === 8) {
        const alpha = Number.parseInt(hex.slice(6, 8), 16) / CHANNEL_MAX;
        if (alpha < 1) return null;
      }
      return { red, green, blue };
    }
    return null;
  }

  const rgbMatch = value.match(/^rgba?\((.+)\)$/u);
  if (!rgbMatch) return null;
  const parts = rgbMatch[1].split(',').map((part) => part.trim());
  if (parts.length !== 3 && parts.length !== 4) return null;

  const red = parseChannel(parts[0] ?? '');
  const green = parseChannel(parts[1] ?? '');
  const blue = parseChannel(parts[2] ?? '');
  if (red === null || green === null || blue === null) return null;
  if ([red, green, blue].some((channel) => channel < 0 || channel > CHANNEL_MAX)) return null;

  if (parts.length === 4) {
    const alphaRaw = parts[3] ?? '';
    const alpha = alphaRaw.endsWith('%')
      ? Number.parseFloat(alphaRaw.slice(0, -1)) / 100
      : Number.parseFloat(alphaRaw);
    if (!Number.isFinite(alpha) || alpha < 1) return null;
  }

  return { red, green, blue };
}

export function resolveComputedCanvas(
  root: HTMLElement,
  readComputedStyle: (element: Element) => CSSStyleDeclaration = (element) =>
    globalThis.getComputedStyle(element),
): OpaqueCanvasRgb | null {
  const background = readComputedStyle(root).backgroundColor;
  return parseOpaqueCssColor(background);
}

export function resolveOpaqueCanvasOrFail(
  preferred: HTMLElement,
  fallback: HTMLElement,
  readComputedStyle: (element: Element) => CSSStyleDeclaration = (element) =>
    globalThis.getComputedStyle(element),
): Effect.Effect<OpaqueCanvasRgb, AppearanceCanvasError> {
  return Effect.gen(function* () {
    const canvas =
      resolveComputedCanvas(preferred, readComputedStyle) ??
      resolveComputedCanvas(fallback, readComputedStyle);
    if (!canvas) {
      return yield* new AppearanceCanvasError({
        message: 'Computed appearance canvas is missing, translucent, or unparseable',
      });
    }
    return canvas;
  });
}

export const fetchAppearance: Effect.Effect<Appearance, CommandError> = runTauriCommandRaw(() =>
  commands.appearanceGet(),
);

export function notifyAppearanceReady(
  appearance: Appearance,
  canvas: OpaqueCanvasRgb,
): Effect.Effect<void, CommandError> {
  return runTauriCommand(() =>
    commands.appearanceReady({
      appearance,
      canvas,
    }),
  ).pipe(Effect.asVoid);
}

export function appearancesEqual(left: Appearance, right: Appearance): boolean {
  return left.designTheme === right.designTheme && left.colorMode === right.colorMode;
}

export function persistAppearanceSelection(
  request: AppearanceSetRequest,
): Effect.Effect<void, CommandError> {
  return runTauriCommand(() => commands.appearanceSet(request)).pipe(Effect.asVoid);
}

/** Fetch Appearance and resolve the hydrated opaque canvas without fabricating fallbacks. */
export const bootstrapAppearance: Effect.Effect<
  BootstrappedAppearance,
  CommandError | AppearanceCanvasError
> = Effect.gen(function* () {
  const root = document.documentElement;
  const appearance = yield* fetchAppearance;
  applyAppearanceRootAttributes(root, appearance);
  const canvas = yield* resolveOpaqueCanvasOrFail(document.body, root);
  return { appearance, canvas };
});
