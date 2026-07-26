import { readFileSync } from 'node:fs';

import { expect, test } from '@rstest/core';

import {
  appearanceSemanticColors,
  braunDarkCanvasHex,
  braunFonts,
  braunLightCanvasHex,
  braunRadii,
  breakpoints,
  controlRoomDarkCanvasHex,
  controlRoomLightCanvasHex,
  durations,
  easings,
  fontSizes,
  fontWeights,
  fonts,
  lineHeights,
  radii,
  rawColors,
  requiredSemanticColorRoles,
  semanticColorHex,
  shadows,
  spacing,
  zIndex,
  type SemanticColorRole,
} from '../src/styles/theme-tokens';

interface Rgb {
  readonly r: number;
  readonly g: number;
  readonly b: number;
  readonly a: number;
}

const NORMAL_TEXT_PAIRS = [
  ['onBackground', 'background'],
  ['onSurface', 'surface'],
  ['onSurfaceVariant', 'surfaceVariant'],
  ['onPrimary', 'primary'],
  ['onPrimaryContainer', 'primaryContainer'],
  ['onSecondary', 'secondary'],
  ['onSecondaryContainer', 'secondaryContainer'],
  ['onTertiary', 'tertiary'],
  ['onTertiaryContainer', 'tertiaryContainer'],
  ['onError', 'error'],
  ['onErrorContainer', 'errorContainer'],
  ['onWarning', 'warning'],
  ['onWarningContainer', 'warningContainer'],
  ['onSuccess', 'success'],
  ['onSuccessContainer', 'successContainer'],
  ['onInfo', 'info'],
  ['onInfoContainer', 'infoContainer'],
  ['onNeutral', 'neutral'],
  ['onNeutralContainer', 'neutralContainer'],
] as const satisfies readonly (readonly [SemanticColorRole, SemanticColorRole])[];

const FOCUS_ADJACENT_SURFACES = [
  'background',
  'surface',
  'surfaceContainer',
  'surfaceContainerHigh',
  'surfaceContainerHighest',
  'surfaceContainerLow',
  'surfaceContainerLowest',
  'surfaceVariant',
  'materialSurfaceRaised',
  'materialSurfaceRecessed',
  'materialSurfaceAcrylic',
  'materialSurfaceGlass',
  'materialSurfaceKey',
  'materialSurfaceKeyHover',
  'materialSurfacePressed',
] as const satisfies readonly SemanticColorRole[];

const TRANSLUCENT_MATERIAL_ROLES = [
  'materialSurfaceAcrylic',
  'materialSurfaceGlass',
] as const satisfies readonly SemanticColorRole[];

function channelToLinear(channel: number): number {
  const value = channel / 255;
  return value <= 0.040_45 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
}

function relativeLuminance(rgb: Rgb): number {
  const r = channelToLinear(rgb.r);
  const g = channelToLinear(rgb.g);
  const b = channelToLinear(rgb.b);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrastRatio(foreground: Rgb, background: Rgb): number {
  const lighter = Math.max(relativeLuminance(foreground), relativeLuminance(background));
  const darker = Math.min(relativeLuminance(foreground), relativeLuminance(background));
  return (lighter + 0.05) / (darker + 0.05);
}

function parseCssColor(value: string): Rgb {
  const trimmed = value.trim().toLowerCase();
  if (trimmed.startsWith('#')) {
    const hex = trimmed.slice(1);
    if (hex.length === 3 || hex.length === 4) {
      const r = Number.parseInt(hex[0]! + hex[0]!, 16);
      const g = Number.parseInt(hex[1]! + hex[1]!, 16);
      const b = Number.parseInt(hex[2]! + hex[2]!, 16);
      const a = hex.length === 4 ? Number.parseInt(hex[3]! + hex[3]!, 16) / 255 : 1;
      return { r, g, b, a };
    }
    if (hex.length === 6 || hex.length === 8) {
      const r = Number.parseInt(hex.slice(0, 2), 16);
      const g = Number.parseInt(hex.slice(2, 4), 16);
      const b = Number.parseInt(hex.slice(4, 6), 16);
      const a = hex.length === 8 ? Number.parseInt(hex.slice(6, 8), 16) / 255 : 1;
      return { r, g, b, a };
    }
    throw new Error(`Unsupported hex color: ${value}`);
  }

  const rgbMatch = trimmed.match(/^rgba?\((.+)\)$/u);
  if (!rgbMatch) throw new Error(`Unsupported color format: ${value}`);
  const parts = rgbMatch[1]!.split(/[\s,/]+/u).filter(Boolean);
  if (parts.length < 3) throw new Error(`Unsupported color format: ${value}`);

  const parseChannel = (raw: string): number => {
    if (raw.endsWith('%')) {
      return Math.round((Number.parseFloat(raw) / 100) * 255);
    }
    return Number.parseFloat(raw);
  };

  const r = parseChannel(parts[0]!);
  const g = parseChannel(parts[1]!);
  const b = parseChannel(parts[2]!);
  let a = 1;
  if (parts[3] !== undefined) {
    a = parts[3].endsWith('%') ? Number.parseFloat(parts[3]) / 100 : Number.parseFloat(parts[3]);
  }
  return { r, g, b, a };
}

function alphaComposite(foreground: Rgb, background: Rgb): Rgb {
  const alpha = foreground.a + background.a * (1 - foreground.a);
  if (alpha === 0) return { r: 0, g: 0, b: 0, a: 0 };
  return {
    r: Math.round(
      (foreground.r * foreground.a + background.r * background.a * (1 - foreground.a)) / alpha,
    ),
    g: Math.round(
      (foreground.g * foreground.a + background.g * background.a * (1 - foreground.a)) / alpha,
    ),
    b: Math.round(
      (foreground.b * foreground.a + background.b * background.a * (1 - foreground.a)) / alpha,
    ),
    a: alpha,
  };
}

function resolveOpaqueColor(value: string, underlay: Rgb): Rgb {
  const parsed = parseCssColor(value);
  if (parsed.a >= 1) return { ...parsed, a: 1 };
  return { ...alphaComposite(parsed, underlay), a: 1 };
}

test('raw palette values match the Control Room contract', () => {
  expect(rawColors.neutral['975']).toBe('#05060a');
  expect(rawColors.indigo['600']).toBe('#4f46e5');
  expect(rawColors.teal['400']).toBe('#4fe3b1');
  expect(rawColors.amber['400']).toBe('#f6c768');
  expect(rawColors.red['400']).toBe('#ff6b7a');
});

test('semantic colors resolve to the same hex values as before', () => {
  expect(semanticColorHex.primary).toBe('#4f46e5');
  expect(semanticColorHex.background).toBe('#05060a');
  expect(semanticColorHex.onSurface).toBe('#f3f6ff');
  expect(semanticColorHex.surfaceContainerLowest).toBe('#040508');
  expect(semanticColorHex.surfaceTint).toBe(semanticColorHex.primary);
});

test('scale tokens preserve prior keys and values', () => {
  expect(spacing['3_5']).toBe('0.875rem');
  expect(spacing.md).toBe('1rem');
  expect(fontSizes['14']).toBe('0.875rem');
  expect(fontWeights.bold).toBe('700');
  expect(lineHeights['20']).toBe('1.25rem');
  expect(radii['2xl']).toBe('1rem');
  expect(shadows.md).toContain('0.35');
  expect(zIndex['50']).toBe('50');
  expect(durations['200']).toBe('200ms');
  expect(easings.standard).toBe('cubic-bezier(0.2, 0, 0, 1)');
  expect(breakpoints.sm).toBe('640px');
  expect(fonts.sans).toContain('Inter Variable');
  expect(fonts.body).toContain('Inter Variable');
  expect(fonts.display).toContain('Space Grotesk Variable');
  expect(fonts.readout).toContain('monospace');
  expect(durations.interaction).toBe('150ms');
  expect(durations.appearance).toBe('200ms');
  expect(durations.mechanical).toBe('120ms');
  expect(radii.panel).toBe('1rem');
});

test('every appearance provides the complete semantic color role set', () => {
  for (const theme of Object.keys(
    appearanceSemanticColors,
  ) as (keyof typeof appearanceSemanticColors)[]) {
    for (const mode of ['light', 'dark'] as const) {
      const table = appearanceSemanticColors[theme][mode];
      for (const role of requiredSemanticColorRoles) {
        expect(table[role], `${theme}/${mode} missing ${role}`).toBeTypeOf('string');
        expect(table[role].length, `${theme}/${mode} empty ${role}`).toBeGreaterThan(0);
      }
    }
  }
});

test('appearance canvas anchors match the production contract', () => {
  expect(controlRoomDarkCanvasHex).toBe('#05060a');
  expect(controlRoomLightCanvasHex).toBe('#f6f7ff');
  expect(braunLightCanvasHex).toBe('#fcf8f8');
  expect(braunDarkCanvasHex).toBe('#0c0e12');
  expect(appearanceSemanticColors['control-room'].light.primary).toBe('#4f46e5');
  expect(appearanceSemanticColors.braun.light.primary).toBe('#c2410c');
  expect(appearanceSemanticColors.braun.dark.primary).toBe('#f97316');
  expect(appearanceSemanticColors['control-room'].dark.focusRing).toContain('#');
  expect(appearanceSemanticColors.braun.light.focusRing).toBe('#c2410c');
  expect(appearanceSemanticColors.braun.dark.focusRing).toBe('#fb923c');
});

test('status families keep Control Room teal/blue and Braun emerald/cyan language', () => {
  expect(appearanceSemanticColors['control-room'].dark.success).toBe('#4fe3b1');
  expect(appearanceSemanticColors['control-room'].dark.info).toBe('#818cf8');
  expect(appearanceSemanticColors.braun.light.success).toBe('#047857');
  expect(appearanceSemanticColors.braun.light.info).toBe('#0e7490');
  expect(appearanceSemanticColors.braun.dark.success).toBe('#34d399');
  expect(appearanceSemanticColors.braun.dark.info).toBe('#22d3ee');
});

test('status indicators keep family hue anchors across appearances', () => {
  for (const theme of Object.keys(
    appearanceSemanticColors,
  ) as (keyof typeof appearanceSemanticColors)[]) {
    for (const mode of ['light', 'dark'] as const) {
      const table = appearanceSemanticColors[theme][mode];
      expect(table.successIndicator, `${theme}/${mode} successIndicator`).toBe(table.success);
      expect(table.infoIndicator, `${theme}/${mode} infoIndicator`).toBe(table.info);
      expect(table.warningIndicator, `${theme}/${mode} warningIndicator`).toBe(table.warning);
      expect(table.errorIndicator, `${theme}/${mode} errorIndicator`).toBe(table.error);
      expect(table.neutralIndicator, `${theme}/${mode} neutralIndicator`).toBe(table.neutral);
    }
  }

  expect(appearanceSemanticColors['control-room'].dark.successIndicator).toBe('#4fe3b1');
  expect(appearanceSemanticColors['control-room'].dark.infoIndicator).toBe('#818cf8');
  expect(appearanceSemanticColors['control-room'].dark.warningIndicator).toBe('#f6c768');
  expect(appearanceSemanticColors['control-room'].dark.errorIndicator).toBe('#ff6b7a');
  expect(appearanceSemanticColors['control-room'].dark.neutralIndicator).toBe('#aeb8cc');

  expect(appearanceSemanticColors.braun.light.successIndicator).toBe('#047857');
  expect(appearanceSemanticColors.braun.light.infoIndicator).toBe('#0e7490');
  expect(appearanceSemanticColors.braun.light.warningIndicator).toBe('#92400e');
  expect(appearanceSemanticColors.braun.light.errorIndicator).toBe('#b91c1c');
  expect(appearanceSemanticColors.braun.light.neutralIndicator).toBe('#4b5563');

  expect(appearanceSemanticColors.braun.dark.successIndicator).toBe('#34d399');
  expect(appearanceSemanticColors.braun.dark.infoIndicator).toBe('#22d3ee');
  expect(appearanceSemanticColors.braun.dark.warningIndicator).toBe('#fbbf24');
  expect(appearanceSemanticColors.braun.dark.errorIndicator).toBe('#f87171');
  expect(appearanceSemanticColors.braun.dark.neutralIndicator).toBe('#d1d5db');
});

test('Braun fonts and radii stay distinct from Control Room', () => {
  expect(braunFonts.body).toContain('Archivo Variable');
  expect(braunFonts.display).toContain('Archivo Variable');
  expect(braunFonts.readout).toContain('JetBrains Mono Variable');
  expect(braunRadii.panel).toBe('0.5rem');
  expect(braunRadii.control).toBe('0.375rem');
});

test('local font packages are bundled and network fonts are rejected', () => {
  const packageJson = JSON.parse(readFileSync('package.json', 'utf8')) as {
    dependencies: Record<string, string>;
  };
  const indexSource = readFileSync('src/index.tsx', 'utf8');
  const e2eSource = readFileSync('e2e/app/index.tsx', 'utf8');

  expect(packageJson.dependencies['@fontsource-variable/inter']).toBeTypeOf('string');
  expect(packageJson.dependencies['@fontsource-variable/space-grotesk']).toBeTypeOf('string');
  expect(packageJson.dependencies['@fontsource-variable/archivo']).toBeTypeOf('string');
  expect(packageJson.dependencies['@fontsource-variable/jetbrains-mono']).toBeTypeOf('string');

  for (const source of [indexSource, e2eSource]) {
    expect(source).toContain('@fontsource-variable/inter');
    expect(source).toContain('@fontsource-variable/space-grotesk');
    expect(source).toContain('@fontsource-variable/archivo');
    expect(source).toContain('@fontsource-variable/jetbrains-mono');
    expect(source).not.toMatch(/fonts\.googleapis|fonts\.gstatic|typekit\.net/u);
  }
});

test('panda config emits named themes and explicit color-mode conditions', () => {
  const pandaConfig = readFileSync('panda.config.ts', 'utf8');
  expect(pandaConfig).toContain("themes: ['control-room', 'braun']");
  expect(pandaConfig).toContain("light: '[data-color-mode=light] &'");
  expect(pandaConfig).toContain("dark: '[data-color-mode=dark] &'");
  expect(pandaConfig).toContain("'control-room':");
  expect(pandaConfig).toContain('braun:');
  expect(pandaConfig).not.toMatch(/'h1, h2, h3, h4, h5, h6':\s*\{[^}]*fontFamily:\s*'display'/u);
});

test('normal-text semantic pairs meet 4.5:1 contrast across all appearances', () => {
  for (const theme of Object.keys(
    appearanceSemanticColors,
  ) as (keyof typeof appearanceSemanticColors)[]) {
    for (const mode of ['light', 'dark'] as const) {
      const table = appearanceSemanticColors[theme][mode];
      const canvas = parseCssColor(table.background);
      for (const [foregroundRole, backgroundRole] of NORMAL_TEXT_PAIRS) {
        const foreground = resolveOpaqueColor(table[foregroundRole], canvas);
        const background = resolveOpaqueColor(table[backgroundRole], canvas);
        const ratio = contrastRatio(foreground, background);
        expect(
          ratio,
          `${theme}/${mode} ${foregroundRole} on ${backgroundRole} contrast ${ratio.toFixed(2)}`,
        ).toBeGreaterThanOrEqual(4.5);
      }
    }
  }
});

test('focusRing meets 3:1 contrast against adjacent surfaces', () => {
  for (const theme of Object.keys(
    appearanceSemanticColors,
  ) as (keyof typeof appearanceSemanticColors)[]) {
    for (const mode of ['light', 'dark'] as const) {
      const table = appearanceSemanticColors[theme][mode];
      const canvas = parseCssColor(table.background);
      const focus = resolveOpaqueColor(table.focusRing, canvas);
      for (const surfaceRole of FOCUS_ADJACENT_SURFACES) {
        const surface = resolveOpaqueColor(table[surfaceRole], canvas);
        const ratio = contrastRatio(focus, surface);
        expect(
          ratio,
          `${theme}/${mode} focusRing on ${surfaceRole} contrast ${ratio.toFixed(2)}`,
        ).toBeGreaterThanOrEqual(3);
      }
    }
  }
});

test('translucent materials are alpha-composited before contrast measurement', () => {
  for (const theme of Object.keys(
    appearanceSemanticColors,
  ) as (keyof typeof appearanceSemanticColors)[]) {
    for (const mode of ['light', 'dark'] as const) {
      const table = appearanceSemanticColors[theme][mode];
      const canvas = parseCssColor(table.background);
      const surface = parseCssColor(table.surface);
      for (const role of TRANSLUCENT_MATERIAL_ROLES) {
        const material = parseCssColor(table[role]);
        if (material.a >= 1) continue;
        const onCanvas = alphaComposite(material, canvas);
        const onSurface = alphaComposite(material, surface);
        const textOnCanvas = contrastRatio(resolveOpaqueColor(table.onSurface, canvas), {
          ...onCanvas,
          a: 1,
        });
        const textOnSurface = contrastRatio(resolveOpaqueColor(table.onSurface, canvas), {
          ...onSurface,
          a: 1,
        });
        expect(
          textOnCanvas,
          `${theme}/${mode} onSurface over ${role}+canvas contrast ${textOnCanvas.toFixed(2)}`,
        ).toBeGreaterThanOrEqual(4.5);
        expect(
          textOnSurface,
          `${theme}/${mode} onSurface over ${role}+surface contrast ${textOnSurface.toFixed(2)}`,
        ).toBeGreaterThanOrEqual(4.5);
      }
    }
  }
});

const STATUS_INDICATOR_ROLES = [
  'successIndicator',
  'infoIndicator',
  'warningIndicator',
  'errorIndicator',
  'neutralIndicator',
] as const satisfies readonly SemanticColorRole[];

test('status indicators meet 3:1 contrast against adjacent surfaces', () => {
  for (const theme of Object.keys(
    appearanceSemanticColors,
  ) as (keyof typeof appearanceSemanticColors)[]) {
    for (const mode of ['light', 'dark'] as const) {
      const table = appearanceSemanticColors[theme][mode];
      const canvas = parseCssColor(table.background);
      for (const indicatorRole of STATUS_INDICATOR_ROLES) {
        const indicator = resolveOpaqueColor(table[indicatorRole], canvas);
        for (const surfaceRole of FOCUS_ADJACENT_SURFACES) {
          const surface = resolveOpaqueColor(table[surfaceRole], canvas);
          const ratio = contrastRatio(indicator, surface);
          expect(
            ratio,
            `${theme}/${mode} ${indicatorRole} on ${surfaceRole} contrast ${ratio.toFixed(2)}`,
          ).toBeGreaterThanOrEqual(3);
        }
      }
    }
  }
});
