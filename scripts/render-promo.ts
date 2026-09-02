#!/usr/bin/env bun
/**
 * Renders JellyPilot promotional artwork from the real app screenshots.
 *
 * Composes light-canvas SVG scenes (brand lockup, kickers, tilted screenshot
 * cards, grain) and rasterizes them with sharp: WebP for README/promotional
 * artwork and PNG for the GitHub social preview. Screenshot cards keep
 * the source window aspect ratio and a small corner radius so the app's own
 * window silhouette stays visible; cards separate through shadow, not an
 * outline, and bleed to the canvas edges so the UI stays readable at README
 * sizes. Promotional outputs land in `assets/promo/`; optimized README
 * screenshots land in `assets/screenshots/`.
 */

import { mkdir, readFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';

import sharp from 'sharp';

const HERE = import.meta.dirname;
const REPO = resolve(HERE, '..');
const OUT = resolve(REPO, 'assets/promo');
const SHOTS_DIR = resolve(REPO, 'assets/screenshots');
const MARK = resolve(REPO, 'assets/jellypilot-banner-mark.png');
const FONTS = resolve(REPO, 'crates/jellypilot-ui/assets/fonts');
const OUTPUT_SCALE = 2;

// Mirrors crates/jellypilot-ui tokens (light palette): paper canvas, dark ink,
// indigo brand accents. Shadows use the dark primary-container navy so they
// read as elevation on paper rather than dirt.
const COLORS = {
  canvas: '#f3f6ff',
  surface: '#ffffff',
  ink: '#05060a',
  muted: '#5c6c8c',
  faint: '#aeb8cc',
  primary: '#4f46e5',
  secondary: '#4f46e5',
  outline: '#aeb8cc',
  cardBack: '#e0e2ff',
} as const;

const FONT_FILES = {
  grotesk: join(FONTS, 'SpaceGrotesk-Variable.ttf'),
  inter: join(FONTS, 'Inter-Variable.ttf'),
} as const;

// Source window aspect ratios: full shell 2623:2135, Control-Only 840:1330.
const FULL_RATIO = 2623 / 2135;
const CONTROL_RATIO = 840 / 1330;
const CARD_RADIUS = 6;

const SHOT_FILES = {
  darkHome: 'Screenshot from 2026-09-02 17-30-56.png',
  darkSeries: 'Screenshot from 2026-09-02 17-29-45.png',
  lightMovies: 'Screenshot from 2026-09-02 17-29-31.png',
  controlDark: 'Screenshot from 2026-09-02 17-31-01.png',
} as const;

interface Card {
  readonly id: string;
  readonly shot: keyof typeof SHOT_FILES;
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
  readonly angle: number;
}

interface Scene {
  readonly name: string;
  readonly format: 'png' | 'webp';
  readonly width: number;
  readonly height: number;
  readonly svg: (assets: Assets) => string;
}

interface Assets {
  readonly mark: string;
  readonly shots: Readonly<Record<keyof typeof SHOT_FILES, string>>;
  readonly grotesk: string;
  readonly inter: string;
}

interface ReadmeScreenshot {
  readonly name: string;
  readonly shot: keyof typeof SHOT_FILES;
  readonly width: number;
  readonly height: number;
  readonly radius: number;
  readonly padding: number;
}

const fullCard = (
  id: string,
  shot: keyof typeof SHOT_FILES,
  x: number,
  y: number,
  width: number,
  angle: number,
): Card => ({ id, shot, x, y, width, height: Math.round(width / FULL_RATIO), angle });

const controlCard = (
  id: string,
  shot: keyof typeof SHOT_FILES,
  x: number,
  y: number,
  width: number,
  angle: number,
): Card => ({ id, shot, x, y, width, height: Math.round(width / CONTROL_RATIO), angle });

const escapeXml = (value: string): string =>
  value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#x27;');

const dataUri = (contents: Uint8Array, mime = 'image/png'): string =>
  `data:${mime};base64,${Buffer.from(contents).toString('base64')}`;

const fontDefs = (assets: Assets): string => `<style>
  @font-face { font-family: 'Space Grotesk'; src: url('${assets.grotesk}') format('truetype'); }
  @font-face { font-family: 'Inter'; src: url('${assets.inter}') format('truetype'); }
</style>`;

const defs = (assets: Assets): string => `<defs>
  ${fontDefs(assets)}
  <radialGradient id="washPrimary" cx="50%" cy="50%" r="50%">
    <stop offset="0%" stop-color="${COLORS.primary}" stop-opacity="0.16"/>
    <stop offset="100%" stop-color="${COLORS.primary}" stop-opacity="0"/>
  </radialGradient>
  <radialGradient id="washSecondary" cx="50%" cy="50%" r="50%">
    <stop offset="0%" stop-color="#818cf8" stop-opacity="0.14"/>
    <stop offset="100%" stop-color="#818cf8" stop-opacity="0"/>
  </radialGradient>
  <radialGradient id="edgeShade" cx="50%" cy="46%" r="78%">
    <stop offset="66%" stop-color="${COLORS.muted}" stop-opacity="0"/>
    <stop offset="100%" stop-color="${COLORS.muted}" stop-opacity="0.1"/>
  </radialGradient>
  <pattern id="grid" width="44" height="44" patternUnits="userSpaceOnUse">
    <path d="M44 0H0V44" fill="none" stroke="${COLORS.primary}" stroke-opacity="0.05" stroke-width="1"/>
  </pattern>
  <pattern id="dots" width="20" height="20" patternUnits="userSpaceOnUse">
    <circle cx="2" cy="2" r="1.4" fill="${COLORS.primary}" fill-opacity="0.1"/>
  </pattern>
  <filter id="shadow" x="-20%" y="-20%" width="140%" height="150%">
    <feDropShadow dx="0" dy="22" stdDeviation="28" flood-color="#1b1c3b" flood-opacity="0.32"/>
    <feDropShadow dx="0" dy="4" stdDeviation="7" flood-color="#1b1c3b" flood-opacity="0.2"/>
  </filter>
  <filter id="grain">
    <feTurbulence type="fractalNoise" baseFrequency="0.8" numOctaves="2" stitchTiles="stitch"/>
    <feColorMatrix type="matrix" values="0 0 0 0 1 0 0 0 0 1 0 0 0 0 1 0 0 0 0.05 0"/>
    <feComposite operator="in" in2="SourceGraphic"/>
  </filter>
</defs>`;

const background = (width: number, height: number): string => `
  <rect width="${width}" height="${height}" fill="${COLORS.canvas}"/>
  <ellipse cx="${width * 0.92}" cy="${-height * 0.12}" rx="${width * 0.52}" ry="${height * 0.62}" fill="url(#washPrimary)"/>
  <ellipse cx="${-width * 0.06}" cy="${height * 1.05}" rx="${width * 0.45}" ry="${height * 0.55}" fill="url(#washSecondary)"/>
  <rect width="${width}" height="${height}" fill="url(#grid)"/>
  <rect x="0" y="0" width="${width * 0.22}" height="${height * 0.2}" fill="url(#dots)"/>
  <rect x="${width * 0.78}" y="${height * 0.8}" width="${width * 0.22}" height="${height * 0.2}" fill="url(#dots)"/>
  <rect width="${width}" height="${height}" fill="url(#edgeShade)"/>`;

const grainOverlay = (width: number, height: number): string =>
  `<rect width="${width}" height="${height}" filter="url(#grain)" pointer-events="none"/>`;

const lockup = (assets: Assets, x: number, y: number, size = 56): string => `
  <image href="${assets.mark}" x="${x}" y="${y}" width="${size}" height="${size}"/>
  <text x="${x + size + 16}" y="${y + size * 0.72}" font-family="Space Grotesk" font-size="${size * 0.52}" font-weight="600" fill="${COLORS.ink}">JellyPilot</text>`;

const kicker = (label: string, x: number, y: number): string => `
  <rect x="${x}" y="${y - 14}" width="26" height="4" rx="2" fill="${COLORS.primary}"/>
  <text x="${x + 36}" y="${y - 6}" font-family="Inter" font-size="14" font-weight="500" letter-spacing="2.6" fill="${COLORS.primary}">${escapeXml(label)}</text>`;

const headline = (
  lines: readonly string[],
  x: number,
  y: number,
  size: number,
  lineHeight: number,
): string =>
  lines
    .map(
      (line, index) =>
        `<text x="${x}" y="${y + index * lineHeight}" font-family="Space Grotesk" font-size="${size}" font-weight="700" letter-spacing="-1.4" fill="${COLORS.ink}">${escapeXml(line)}</text>`,
    )
    .join('\n');

const body = (lines: readonly string[], x: number, y: number, size = 20, lineHeight = 31): string =>
  lines
    .map(
      (line, index) =>
        `<text x="${x}" y="${y + index * lineHeight}" font-family="Inter" font-size="${size}" font-weight="400" fill="${COLORS.muted}">${escapeXml(line)}</text>`,
    )
    .join('\n');

const chipWidth = (label: string): number => Math.round(label.length * 8) + 36;

const chip = (label: string, x: number, y: number): string => {
  const width = chipWidth(label);
  return `
  <g transform="translate(${x} ${y})">
    <rect width="${width}" height="36" rx="8" fill="${COLORS.cardBack}"/>
    <text x="18" y="23.5" font-family="Inter" font-size="13" font-weight="600" letter-spacing="0.2" fill="${COLORS.primary}">${escapeXml(label)}</text>
  </g>`;
};

const chipRow = (labels: readonly string[], x: number, y: number): string => {
  let cursor = x;
  const parts: string[] = [];
  for (const label of labels) {
    parts.push(chip(label, cursor, y));
    cursor += chipWidth(label) + 10;
  }
  return parts.join('\n');
};

// Cards render the screenshot at its native window aspect: no `slice` crop, so
// the app's own title bar and player bar stay inside the frame. The radius is
// deliberately small and cards carry no outline — shadow alone separates the
// layers, so the window's own silhouette stays honest.
const screenCard = (assets: Assets, card: Card): string => {
  const { id, x, y, width, height, angle } = card;
  const cx = width / 2;
  const cy = height / 2;
  return `
  <defs><clipPath id="clip-${id}"><rect width="${width}" height="${height}" rx="${CARD_RADIUS}"/></clipPath></defs>
  <g transform="translate(${x + 14} ${y + 18}) rotate(${(-angle * 1.7).toFixed(2)} ${cx} ${cy})">
    <rect width="${width}" height="${height}" rx="${CARD_RADIUS}" fill="${COLORS.cardBack}"/>
  </g>
  <g transform="translate(${x} ${y}) rotate(${angle} ${cx} ${cy})">
    <rect width="${width}" height="${height}" rx="${CARD_RADIUS}" fill="${COLORS.canvas}" filter="url(#shadow)"/>
    <image href="${assets.shots[card.shot]}" width="${width}" height="${height}" clip-path="url(#clip-${id})"/>
  </g>`;
};

const heroSvg = (
  assets: Assets,
): string => `<svg xmlns="http://www.w3.org/2000/svg" width="1600" height="900" viewBox="0 0 1600 900">
${defs(assets)}
${background(1600, 900)}
${lockup(assets, 88, 64)}
${kicker('CROSS-PLATFORM · NATIVE', 88, 194)}
${headline(['Your library.', 'Your MPV.'], 88, 272, 84, 96)}
${body(['A custom-drawn companion for Jellyfin and Emby.', 'Playback runs in your own MPV — direct play,', 'original quality.'], 88, 496)}
${chipRow(['Jellyfin + Emby', 'External MPV', 'Windows · macOS · Linux'], 88, 586)}
${screenCard(assets, fullCard('hero-product', 'darkHome', 720, 116, 840, 0))}
${grainOverlay(1600, 900)}
</svg>`;

const marqueeSvg = (
  assets: Assets,
): string => `<svg xmlns="http://www.w3.org/2000/svg" width="1400" height="560" viewBox="0 0 1400 560">
${defs(assets)}
${background(1400, 560)}
${lockup(assets, 72, 52, 48)}
${kicker('JELLYFIN · EMBY · MPV', 72, 154)}
${headline(['Your library.', 'Your MPV.'], 72, 216, 54, 64)}
${body(['One custom-drawn native app.', 'Direct play, always.'], 72, 362, 17, 26)}
${screenCard(assets, fullCard('marq-left', 'darkSeries', 676, 96, 470, -2.8))}
${screenCard(assets, fullCard('marq-right', 'lightMovies', 958, 80, 470, 2.4))}
${screenCard(assets, controlCard('marq-control', 'controlDark', 1164, 126, 192, 0.6))}
${grainOverlay(1400, 560)}
</svg>`;

const smallSvg = (
  assets: Assets,
): string => `<svg xmlns="http://www.w3.org/2000/svg" width="440" height="280" viewBox="0 0 440 280">
${defs(assets)}
${background(440, 280)}
<image href="${assets.mark}" x="188" y="40" width="64" height="64"/>
<text x="220" y="142" text-anchor="middle" font-family="Space Grotesk" font-size="27" font-weight="600" fill="${COLORS.ink}">JellyPilot</text>
<text x="220" y="170" text-anchor="middle" font-family="Inter" font-size="13" font-weight="500" letter-spacing="1.8" fill="${COLORS.primary}">EXTERNAL MPV PLAYBACK</text>
${screenCard(assets, controlCard('small-peek', 'controlDark', 152, 188, 136, 0))}
${grainOverlay(440, 280)}
</svg>`;

const socialPreviewSvg = (
  assets: Assets,
): string => `<svg xmlns="http://www.w3.org/2000/svg" width="640" height="320" viewBox="0 0 640 320">
${defs(assets)}
${background(640, 320)}
${lockup(assets, 40, 30, 36)}
${kicker('JELLYFIN · EMBY · MPV', 40, 116)}
${headline(['Your library.', 'Your MPV.'], 40, 166, 40, 47)}
${body(['Native playback through your own MPV.'], 40, 270, 13, 20)}
${screenCard(assets, fullCard('social-product', 'darkHome', 340, 54, 280, 0))}
${grainOverlay(640, 320)}
</svg>`;

const playbackSvg = (
  assets: Assets,
): string => `<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="800" viewBox="0 0 1280 800">
${defs(assets)}
${background(1280, 800)}
${lockup(assets, 88, 64)}
${kicker('PLAYBACK', 88, 192)}
${headline(['Play through', 'your own MPV.'], 88, 266, 66, 76)}
${body(['JSON IPC control of a standalone player.', 'Direct play. Your shaders and scripts.'], 88, 444, 19, 29)}
${chipRow(['Episode queue', 'Intro Skipper', 'External subtitles'], 88, 518)}
${screenCard(assets, fullCard('play-back', 'darkSeries', 640, 180, 690, -1.3))}
${screenCard(assets, controlCard('play-control', 'controlDark', 962, 104, 292, 0.9))}
${grainOverlay(1280, 800)}
</svg>`;

const SCENES: readonly Scene[] = [
  { name: 'hero', format: 'webp', width: 1600, height: 900, svg: heroSvg },
  {
    name: 'social-preview',
    format: 'png',
    width: 640,
    height: 320,
    svg: socialPreviewSvg,
  },
  { name: 'marquee', format: 'webp', width: 1400, height: 560, svg: marqueeSvg },
  { name: 'small', format: 'webp', width: 440, height: 280, svg: smallSvg },
  { name: 'playback', format: 'webp', width: 1280, height: 800, svg: playbackSvg },
];

const README_SCREENSHOTS: readonly ReadmeScreenshot[] = [
  {
    name: 'readme-home',
    shot: 'darkHome',
    width: 2623,
    height: 2135,
    radius: 52,
    padding: 72,
  },
  {
    name: 'readme-library',
    shot: 'lightMovies',
    width: 2623,
    height: 2135,
    radius: 52,
    padding: 72,
  },
  {
    name: 'readme-control',
    shot: 'controlDark',
    width: 840,
    height: 1330,
    radius: 32,
    padding: 44,
  },
];

interface ReadmeRender {
  readonly svg: string;
  readonly width: number;
  readonly height: number;
}

const renderReadmeScreenshot = (assets: Assets, screenshot: ReadmeScreenshot): ReadmeRender => {
  const width = screenshot.width + screenshot.padding * 2;
  const height = screenshot.height + screenshot.padding * 2;
  const shadowOffset = Math.round(screenshot.padding * 0.22);
  const shadowBlur = Math.round(screenshot.padding * 0.24);
  const contactOffset = Math.round(screenshot.padding * 0.06);
  const contactBlur = Math.round(screenshot.padding * 0.07);
  const { padding, radius } = screenshot;

  return {
    width,
    height,
    svg: `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">
  <defs>
    <clipPath id="readme-clip">
      <rect x="${padding}" y="${padding}" width="${screenshot.width}" height="${screenshot.height}" rx="${radius}"/>
    </clipPath>
    <filter id="readme-shadow" x="0" y="0" width="${width}" height="${height}" filterUnits="userSpaceOnUse" color-interpolation-filters="sRGB">
      <feDropShadow in="SourceGraphic" dx="0" dy="${shadowOffset}" stdDeviation="${shadowBlur}" flood-color="#000000" flood-opacity="0.18" result="ambient-shadow"/>
      <feDropShadow in="SourceGraphic" dx="0" dy="${contactOffset}" stdDeviation="${contactBlur}" flood-color="#000000" flood-opacity="0.14" result="contact-shadow"/>
      <feMerge>
        <feMergeNode in="ambient-shadow"/>
        <feMergeNode in="contact-shadow"/>
        <feMergeNode in="SourceGraphic"/>
      </feMerge>
    </filter>
  </defs>
  <g filter="url(#readme-shadow)">
    <image href="${assets.shots[screenshot.shot]}" x="${padding}" y="${padding}" width="${screenshot.width}" height="${screenshot.height}" clip-path="url(#readme-clip)"/>
  </g>
</svg>`,
  };
};

const loadAssets = async (): Promise<Assets> => {
  const [mark, grotesk, inter, ...shots] = await Promise.all([
    readFile(MARK),
    readFile(FONT_FILES.grotesk),
    readFile(FONT_FILES.inter),
    ...Object.values(SHOT_FILES).map((file) => readFile(join(SHOTS_DIR, file))),
  ]);
  const keys = Object.keys(SHOT_FILES) as (keyof typeof SHOT_FILES)[];
  const shotMap = Object.fromEntries(
    keys.map((key, index) => [key, dataUri(shots[index])]),
  ) as Record<keyof typeof SHOT_FILES, string>;
  return {
    mark: dataUri(mark),
    shots: shotMap,
    grotesk: dataUri(grotesk, 'font/ttf'),
    inter: dataUri(inter, 'font/ttf'),
  };
};

const main = async (): Promise<void> => {
  const assets = await loadAssets();
  await mkdir(OUT, { recursive: true });
  for (const scene of SCENES) {
    const outputWidth = scene.width * OUTPUT_SCALE;
    const outputHeight = scene.height * OUTPUT_SCALE;
    const outputPath = join(OUT, `${scene.name}.${scene.format}`);
    const image = sharp(Buffer.from(scene.svg(assets)), { density: 96 * OUTPUT_SCALE })
      .resize(outputWidth, outputHeight, { fit: 'fill' })
      .flatten({ background: COLORS.canvas })
      .toColorspace('srgb');
    if (scene.format === 'webp') {
      image.webp({ quality: 95, smartSubsample: true, effort: 6 });
    } else {
      image.png({ compressionLevel: 9 });
    }
    await image.toFile(outputPath);
    const metadata = await sharp(outputPath).metadata();
    if (
      metadata.format !== scene.format ||
      metadata.width !== outputWidth ||
      metadata.height !== outputHeight ||
      metadata.hasAlpha !== false
    ) {
      throw new Error(
        `Invalid render for ${scene.name}: ${metadata.format} ${metadata.width}x${metadata.height} alpha=${metadata.hasAlpha}`,
      );
    }
    console.log(
      `rendered assets/promo/${scene.name}.${scene.format} (${outputWidth}x${outputHeight})`,
    );
  }

  for (const screenshot of README_SCREENSHOTS) {
    const render = renderReadmeScreenshot(assets, screenshot);
    const webpPath = join(SHOTS_DIR, `${screenshot.name}.webp`);
    await sharp(Buffer.from(render.svg))
      .toColorspace('srgb')
      .webp({ quality: 90, smartSubsample: true, effort: 6 })
      .toFile(webpPath);
    const metadata = await sharp(webpPath).metadata();
    if (
      metadata.format !== 'webp' ||
      metadata.width !== render.width ||
      metadata.height !== render.height ||
      metadata.hasAlpha !== true
    ) {
      throw new Error(
        `Invalid render for ${screenshot.name}: ${metadata.format} ${metadata.width}x${metadata.height} alpha=${metadata.hasAlpha}`,
      );
    }
    console.log(
      `rendered assets/screenshots/${screenshot.name}.webp (${render.width}x${render.height})`,
    );
  }
};

await main();
