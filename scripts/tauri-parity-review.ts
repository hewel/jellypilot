#!/usr/bin/env bun
/**
 * Manual Tauri/WebKit parity-review assistant for issue #141.
 *
 * It controls the real review window through Niri, records human verdicts,
 * and produces a durable-evidence checklist without introducing pixel diffs.
 */
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, dirname, relative, resolve } from 'node:path';

export const REVIEW_WINDOW_TITLE = 'JellyPilot — Panda Review';
export const DEFAULT_ARTIFACT_DIR = '.artifacts/tauri-parity';

export const VIEWPORTS = Object.freeze({
  stress360: Object.freeze({ height: 720, label: '360x720', width: 360 }),
  stress640: Object.freeze({ height: 720, label: '640x720', width: 640 }),
  stress800: Object.freeze({ height: 600, label: '800x600', width: 800 }),
  supported1280: Object.freeze({ height: 720, label: '1280x720', width: 1280 }),
  supported1600: Object.freeze({ height: 900, label: '1600x900', width: 1600 }),
});

type Viewport = (typeof VIEWPORTS)[keyof typeof VIEWPORTS];

const VIEWPORT_ORDER: readonly Viewport[] = [
  VIEWPORTS.stress360,
  VIEWPORTS.stress640,
  VIEWPORTS.stress800,
  VIEWPORTS.supported1280,
  VIEWPORTS.supported1600,
];

export const SCREENS = Object.freeze({
  login: Object.freeze({ label: 'Login', stress: true }),
  operations: Object.freeze({ label: 'Operations Console', stress: true }),
  'library-landing': Object.freeze({ label: 'Library landing', stress: false }),
  'library-browse': Object.freeze({ label: 'Library browse', stress: true }),
  'item-detail': Object.freeze({ label: 'Item detail', stress: true }),
  'series-detail': Object.freeze({ label: 'Series detail', stress: false }),
});

type ScreenId = keyof typeof SCREENS;

export const EVIDENCE_CHECKS = Object.freeze({
  focus: 'Keyboard focus',
  states: 'Disabled, selected, and error states',
  portals: 'Ark overlays and portals',
  overflow: 'Scrolling and overflow',
  responsive: 'Responsive transitions',
  'reduced-motion': 'Reduced motion',
  'data-states': 'Connected/disconnected and loaded/empty/error states',
});

type CheckId = keyof typeof EVIDENCE_CHECKS;

export interface MatrixEntry {
  id: string;
  screen: ScreenId;
  screenLabel: string;
  viewport: string;
  width: number;
  height: number;
}

type Verdict = 'pass' | 'fail' | 'pending';

interface EntryResult {
  checks: string[];
  evidence: string | null;
  notes: string;
  reviewedAt: string | null;
  states: string[];
  verdict: Verdict;
}

export interface ReviewState {
  version: number;
  issue: number;
  pr: number;
  updatedAt: string | null;
  entries: Record<string, EntryResult>;
}

function requiredViewports(screen: ScreenId): readonly Viewport[] {
  const supported = [VIEWPORTS.supported1280, VIEWPORTS.supported1600];
  if (!SCREENS[screen]?.stress) return supported;
  return [...VIEWPORT_ORDER.slice(0, 3), ...supported];
}

export function matrixEntries(): MatrixEntry[] {
  return (Object.keys(SCREENS) as ScreenId[]).flatMap((screen) =>
    requiredViewports(screen).map((viewport) => ({
      id: `${screen}@${viewport.label}`,
      screen,
      screenLabel: SCREENS[screen].label,
      viewport: viewport.label,
      width: viewport.width,
      height: viewport.height,
    })),
  );
}

export function createInitialState(): ReviewState {
  return {
    version: 1,
    issue: 141,
    pr: 148,
    updatedAt: null,
    entries: Object.fromEntries(
      matrixEntries().map((entry) => [
        entry.id,
        {
          checks: [],
          evidence: null,
          notes: '',
          reviewedAt: null,
          states: [],
          verdict: 'pending',
        },
      ]),
    ),
  };
}

export function parseViewport(value: string): Viewport {
  const viewport = VIEWPORT_ORDER.find((candidate) => candidate.label === value);
  if (!viewport) {
    throw new Error(
      `Unknown viewport '${value}'. Use ${VIEWPORT_ORDER.map((v) => v.label).join(', ')}.`,
    );
  }
  return viewport;
}

export function getMatrixEntry(screen: string, viewport: string): MatrixEntry {
  if (!SCREENS[screen as ScreenId]) {
    throw new Error(`Unknown screen '${screen}'. Use ${Object.keys(SCREENS).join(', ')}.`);
  }
  parseViewport(viewport);
  const entry = matrixEntries().find(
    (candidate) => candidate.screen === screen && candidate.viewport === viewport,
  );
  if (!entry) {
    throw new Error(
      `${SCREENS[screen as ScreenId].label} does not require review at ${viewport} under #141.`,
    );
  }
  return entry;
}

function normalizeList(value: string | undefined): string[] {
  if (!value) return [];
  return [
    ...new Set(
      value
        .split(',')
        .map((item) => item.trim())
        .filter(Boolean),
    ),
  ];
}

interface RecordOptions {
  verdict: string;
  checks?: string;
  states?: string;
  evidence?: string;
  notes?: string;
}

export function recordResult(
  state: ReviewState,
  screen: string,
  viewport: string,
  options: RecordOptions,
): ReviewState {
  const matrixEntry = getMatrixEntry(screen, viewport);
  const verdict = options.verdict;
  if (!['pass', 'fail', 'pending'].includes(verdict)) {
    throw new Error("Verdict must be 'pass', 'fail', or 'pending'.");
  }

  const current = state.entries[matrixEntry.id] ?? createInitialState().entries[matrixEntry.id];
  const checks = options.checks === undefined ? current.checks : normalizeList(options.checks);
  const states = options.states === undefined ? current.states : normalizeList(options.states);
  const evidence = options.evidence ?? current.evidence;

  for (const check of checks) {
    if (!EVIDENCE_CHECKS[check as CheckId]) {
      throw new Error(`Unknown check '${check}'. Use ${Object.keys(EVIDENCE_CHECKS).join(', ')}.`);
    }
  }

  if (verdict === 'pass' && !evidence) {
    throw new Error('A passing verdict requires --evidence or a prior capture.');
  }
  if (verdict === 'pass' && !checks.includes('overflow')) {
    throw new Error("A passing verdict must include the 'overflow' check.");
  }

  return {
    ...state,
    updatedAt: new Date().toISOString(),
    entries: {
      ...state.entries,
      [matrixEntry.id]: {
        checks,
        evidence,
        notes: options.notes ?? current.notes,
        reviewedAt: verdict === 'pending' ? null : new Date().toISOString(),
        states,
        verdict: verdict as Verdict,
      },
    },
  };
}

export function progressSummary(state: ReviewState): {
  failed: number;
  passed: number;
  pending: number;
  total: number;
} {
  const values = matrixEntries().map((entry) => state.entries[entry.id]);
  return {
    failed: values.filter((entry) => entry?.verdict === 'fail').length,
    passed: values.filter((entry) => entry?.verdict === 'pass').length,
    pending: values.filter((entry) => !entry || entry.verdict === 'pending').length,
    total: values.length,
  };
}

export function nextPendingEntry(state: ReviewState): MatrixEntry | null {
  return matrixEntries().find((entry) => state.entries[entry.id]?.verdict !== 'pass') ?? null;
}

function markdownEscape(value: string | undefined | null): string {
  return String(value ?? '')
    .replaceAll('|', String.raw`\|`)
    .replaceAll('\n', '<br>');
}

function evidenceLink(evidence: string | null | undefined): string {
  if (!evidence) return '—';
  if (/^https?:\/\//.test(evidence)) return `[link](${evidence})`;
  return `\`${evidence}\``;
}

export function renderMarkdownReport(state: ReviewState): string {
  const summary = progressSummary(state);
  const lines = [
    '## Tauri/WebKit parity matrix — issue #141',
    '',
    `Progress: **${summary.passed}/${summary.total} passed**, **${summary.failed} failed**, **${summary.pending} pending**.`,
    '',
    '| Screen | Viewport | Verdict | States | Checks | Evidence | Notes |',
    '|---|---:|---|---|---|---|---|',
  ];

  for (const entry of matrixEntries()) {
    const result = state.entries[entry.id];
    const verdict =
      result?.verdict === 'pass' ? 'PASS' : result?.verdict === 'fail' ? 'FAIL' : 'PENDING';
    lines.push(
      `| ${entry.screenLabel} | ${entry.viewport} | ${verdict} | ${markdownEscape(result?.states.join(', ') || '—')} | ${markdownEscape(result?.checks.join(', ') || '—')} | ${evidenceLink(result?.evidence)} | ${markdownEscape(result?.notes || '—')} |`,
    );
  }

  const coveredChecks = new Set(
    Object.values(state.entries)
      .filter((entry) => entry.verdict === 'pass')
      .flatMap((entry) => entry.checks),
  );
  lines.push('', '### Cross-cutting evidence', '');
  for (const [id, label] of Object.entries(EVIDENCE_CHECKS)) {
    lines.push(`- [${coveredChecks.has(id) ? 'x' : ' '}] ${label}`);
  }
  lines.push('', '> A PASS records human review evidence; it is not a pixel-diff assertion.', '');
  return lines.join('\n');
}

function readState(artifactDir: string): ReviewState {
  const statePath = resolve(artifactDir, 'state.json');
  if (!existsSync(statePath)) return createInitialState();
  const parsed = JSON.parse(readFileSync(statePath, 'utf8')) as Partial<ReviewState>;
  if (parsed.version !== 1) throw new Error(`Unsupported state version ${parsed.version}.`);
  const initial = createInitialState();
  return { ...initial, ...parsed, entries: { ...initial.entries, ...parsed.entries } };
}

function writeState(artifactDir: string, state: ReviewState): void {
  mkdirSync(artifactDir, { recursive: true });
  writeFileSync(resolve(artifactDir, 'state.json'), `${JSON.stringify(state, null, 2)}\n`);
}

interface RunOptions {
  capture?: boolean;
}

function run(command: string, args: string[], options: RunOptions = {}): string {
  const result = spawnSync(command, args, {
    encoding: 'utf8',
    stdio: options.capture ? 'pipe' : 'inherit',
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    const detail = options.capture ? `\n${result.stderr || result.stdout}` : '';
    throw new Error(`${command} ${args.join(' ')} failed with status ${result.status}.${detail}`);
  }
  return result.stdout ?? '';
}

interface NiriWindow {
  id: number;
  title: string;
  app_id: string;
  is_floating: boolean;
  layout?: { window_size?: [number, number] };
}

function reviewWindow(): NiriWindow {
  const output = run('niri', ['msg', '--json', 'windows'], { capture: true });
  const windows = JSON.parse(output) as NiriWindow[];
  const matches = windows.filter(
    (window) => window.title === REVIEW_WINDOW_TITLE || window.app_id === 'jellypilot',
  );
  if (matches.length === 0) {
    throw new Error(
      `No '${REVIEW_WINDOW_TITLE}' window found. Start it with bun run review:parity -- launch.`,
    );
  }
  return matches.find((window) => window.title === REVIEW_WINDOW_TITLE) ?? matches[0];
}

function waitForWindowSize(entry: MatrixEntry): NiriWindow {
  const waitBuffer = new Int32Array(new SharedArrayBuffer(4));
  let resized = reviewWindow();
  for (let attempt = 0; attempt < 20; attempt += 1) {
    const [actualWidth, actualHeight] = resized.layout?.window_size ?? [];
    if (actualWidth === entry.width && actualHeight === entry.height) return resized;
    Atomics.wait(waitBuffer, 0, 0, 100);
    resized = reviewWindow();
  }
  const [actualWidth, actualHeight] = resized.layout?.window_size ?? [];
  throw new Error(
    `Niri reported ${actualWidth ?? '?'}x${actualHeight ?? '?'} after requesting ${entry.viewport}.`,
  );
}

function resizeWindow(entry: MatrixEntry): number {
  const window = reviewWindow();
  if (!window.is_floating) {
    run('niri', ['msg', 'action', 'toggle-window-floating', '--id', String(window.id)]);
  }
  run('niri', [
    'msg',
    'action',
    'set-window-width',
    '--id',
    String(window.id),
    String(entry.width),
  ]);
  run('niri', [
    'msg',
    'action',
    'set-window-height',
    '--id',
    String(window.id),
    String(entry.height),
  ]);
  waitForWindowSize(entry);
  run('niri', ['msg', 'action', 'focus-window', '--id', String(window.id)]);
  run('niri', [
    'msg',
    'action',
    'move-floating-window',
    '--id',
    String(window.id),
    '--x',
    '8',
    '--y',
    '8',
  ]);
  return window.id;
}

function timestamp(): string {
  return new Date().toISOString().replaceAll(':', '-').replaceAll('.', '-');
}

function sanitize(value: string): string {
  return value
    .toLowerCase()
    .replaceAll(/[^a-z0-9]+/g, '-')
    .replaceAll(/^-|-$/g, '');
}

function captureWindow(artifactDir: string, entry: MatrixEntry, stateLabel: string): string {
  const windowId = resizeWindow(entry);
  // Niri can report the requested size before WebKit has painted the new viewport.
  // Give the webview one frame budget plus headroom.
  // This prevents evidence from capturing a transient partially repainted surface.
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 250);
  const fileName = [
    entry.screen,
    entry.viewport,
    sanitize(stateLabel || 'default'),
    timestamp(),
  ].join('-');
  const absolute = resolve(artifactDir, 'screenshots', `${fileName}.png`);
  mkdirSync(dirname(absolute), { recursive: true });
  run('niri', ['msg', 'action', 'screenshot-window', '--id', String(windowId), '--path', absolute]);
  return relative(process.cwd(), absolute).replaceAll('\\', '/');
}

interface ParsedArguments {
  flags: Record<string, string | boolean>;
  positional: string[];
}

function parseArguments(args: string[]): ParsedArguments {
  const positional: string[] = [];
  const flags: Record<string, string | boolean> = {};
  for (let index = 0; index < args.length; index += 1) {
    const value = args[index];
    if (!value.startsWith('--')) {
      positional.push(value);
      continue;
    }
    const name = value.slice(2);
    const next = args[index + 1];
    if (next === undefined || next.startsWith('--')) flags[name] = true;
    else {
      flags[name] = next;
      index += 1;
    }
  }
  return { flags, positional };
}

function statusTable(state: ReviewState): string {
  const summary = progressSummary(state);
  const rows = matrixEntries().map((entry) => {
    const verdict = state.entries[entry.id]?.verdict ?? 'pending';
    const marker = verdict === 'pass' ? '✓' : verdict === 'fail' ? '✗' : '·';
    return `${marker} ${entry.id}`;
  });
  return [
    `${summary.passed}/${summary.total} passed, ${summary.failed} failed, ${summary.pending} pending`,
    ...rows,
  ].join('\n');
}

function help(): string {
  return `Tauri/WebKit parity review assistant (#141)

Usage:
  bun run review:parity -- launch
  bun run review:parity -- doctor
  bun run review:parity -- status
  bun run review:parity -- next
  bun run review:parity -- resize <screen> <viewport>
  bun run review:parity -- capture <screen> <viewport> [--state <label>]
  bun run review:parity -- record <screen> <viewport> --verdict pass|fail|pending
    [--states loaded,dialog] [--checks focus,portals,overflow]
    [--evidence <path-or-url>] [--notes <text>]
  bun run review:parity -- report [--out <path>]

Screens: ${Object.keys(SCREENS).join(', ')}
Viewports: ${VIEWPORT_ORDER.map((viewport) => viewport.label).join(', ')}
Checks: ${Object.keys(EVIDENCE_CHECKS).join(', ')}

Artifacts default to ${DEFAULT_ARTIFACT_DIR}; override with --artifact-dir <path>.`;
}

async function main(argv: string[]): Promise<void> {
  const { flags, positional } = parseArguments(argv);
  const command = positional[0] ?? 'help';
  const artifactDir = resolve(String(flags['artifact-dir'] ?? DEFAULT_ARTIFACT_DIR));
  const state = readState(artifactDir);

  if (command === 'help' || command === '--help' || command === '-h') {
    console.log(help());
    return;
  }
  if (command === 'doctor') {
    const window = reviewWindow();
    if (flags.json) {
      console.log(JSON.stringify(window, null, 2));
      return;
    }
    console.log(`Found ${window.title} (window ${window.id}).`);
    console.log(`Artifacts: ${relative(process.cwd(), artifactDir) || '.'}`);
    console.log(`Matrix: ${matrixEntries().length} required screen/viewport checks.`);
    return;
  }
  if (command === 'launch') {
    run('bun', ['run', 'review:panda:tauri']);
    return;
  }
  if (command === 'status') {
    console.log(statusTable(state));
    return;
  }
  if (command === 'next') {
    const entry = nextPendingEntry(state);
    if (!entry) {
      console.log('All required matrix entries pass. Generate the report and attach its evidence.');
      return;
    }
    console.log(`Next: ${entry.screenLabel} at ${entry.viewport}`);
    console.log(`Resize: bun run review:parity -- resize ${entry.screen} ${entry.viewport}`);
    console.log(
      `Capture: bun run review:parity -- capture ${entry.screen} ${entry.viewport} --state <state>`,
    );
    return;
  }
  if (command === 'resize') {
    const entry = getMatrixEntry(positional[1], positional[2]);
    const windowId = resizeWindow(entry);
    console.log(`Window ${windowId} resized to ${entry.viewport} for ${entry.screenLabel}.`);
    return;
  }
  if (command === 'capture') {
    const entry = getMatrixEntry(positional[1], positional[2]);
    const evidence = captureWindow(artifactDir, entry, String(flags.state ?? 'default'));
    const updated = recordResult(state, entry.screen, entry.viewport, {
      evidence,
      verdict: 'pending',
    });
    writeState(artifactDir, updated);
    console.log(`Captured ${evidence}`);
    console.log('Review the image, then use record with a pass or fail verdict.');
    return;
  }
  if (command === 'record') {
    const entry = getMatrixEntry(positional[1], positional[2]);
    const updated = recordResult(state, entry.screen, entry.viewport, {
      checks: typeof flags.checks === 'string' ? flags.checks : undefined,
      evidence: typeof flags.evidence === 'string' ? flags.evidence : undefined,
      notes: typeof flags.notes === 'string' ? flags.notes : undefined,
      states: typeof flags.states === 'string' ? flags.states : undefined,
      verdict: String(flags.verdict ?? ''),
    });
    writeState(artifactDir, updated);
    console.log(`Recorded ${updated.entries[entry.id].verdict} for ${entry.id}.`);
    return;
  }
  if (command === 'report') {
    const report = renderMarkdownReport(state);
    if (typeof flags.out === 'string') {
      const outputPath = resolve(flags.out);
      mkdirSync(dirname(outputPath), { recursive: true });
      writeFileSync(outputPath, report);
      console.log(`Wrote ${relative(process.cwd(), outputPath) || basename(outputPath)}`);
    } else console.log(report);
    return;
  }
  throw new Error(`Unknown command '${command}'.\n\n${help()}`);
}

const isMain = process.argv[1] && resolve(process.argv[1]) === import.meta.filename;
if (isMain) {
  try {
    await main(process.argv.slice(2));
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
