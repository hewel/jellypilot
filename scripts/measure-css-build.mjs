#!/usr/bin/env bun
/**
 * Clean production-build CSS metrics helper.
 * Runs three clean `bun run build`s and reports raw/gzip CSS bytes + median wall time.
 *
 * Usage:
 *   bun scripts/measure-css-build.mjs
 *   bun scripts/measure-css-build.mjs --label slice-1 --out metrics/slice-1.json
 */
import { spawnSync } from 'node:child_process';
import {
  createReadStream,
  existsSync,
  mkdirSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { join, resolve } from 'node:path';
import { pipeline } from 'node:stream/promises';
import { createGzip } from 'node:zlib';

const args = process.argv.slice(2);
const labelIdx = args.indexOf('--label');
const outIdx = args.indexOf('--out');
const label = labelIdx !== -1 ? args[labelIdx + 1] : 'local';
const outPath = outIdx !== -1 ? args[outIdx + 1] : null;
const runs = 3;
const root = process.cwd();
const distDir = join(root, 'dist');

function listCssFiles(dir) {
  if (!existsSync(dir)) return [];
  const out = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) out.push(...listCssFiles(full));
    else if (entry.isFile() && entry.name.endsWith('.css')) out.push(full);
  }
  return out;
}

async function gzipSize(filePath) {
  let size = 0;
  const gzip = createGzip();
  gzip.on('data', (chunk) => {
    size += chunk.length;
  });
  await pipeline(createReadStream(filePath), gzip);
  return size;
}

async function measureCss() {
  const files = listCssFiles(distDir);
  let raw = 0;
  let gzip = 0;
  for (const file of files) {
    raw += statSync(file).size;
    gzip += await gzipSize(file);
  }
  return { raw, gzip, files: files.map((f) => f.replace(`${root}/`, '')) };
}

function runBuild() {
  rmSync(distDir, { recursive: true, force: true });
  const start = performance.now();
  const result = spawnSync('bun', ['run', 'build'], {
    cwd: root,
    encoding: 'utf8',
    env: process.env,
  });
  const wallMs = Math.round(performance.now() - start);
  if (result.status !== 0) {
    console.error(result.stdout);
    console.error(result.stderr);
    throw new Error(`build failed with status ${result.status}`);
  }
  return wallMs;
}

const runsData = [];
for (let i = 0; i < runs; i += 1) {
  const wallMs = runBuild();
  const css = await measureCss();
  runsData.push({ wallMs, cssRawBytes: css.raw, cssGzipBytes: css.gzip, files: css.files });
  console.log(
    `run=${i + 1} wallMs=${wallMs} cssRawBytes=${css.raw} cssGzipBytes=${css.gzip} files=${css.files.length}`,
  );
}

const sorted = runsData.toSorted((a, b) => a.wallMs - b.wallMs);
const median = sorted[Math.floor(sorted.length / 2)];
const report = {
  label,
  capturedAt: new Date().toISOString(),
  command: 'bun run build (clean dist, 3 runs)',
  runs: runsData.map(({ wallMs, cssRawBytes, cssGzipBytes }) => ({
    wallMs,
    cssRawBytes,
    cssGzipBytes,
  })),
  medianWallMs: median.wallMs,
  cssRawBytes: median.cssRawBytes,
  cssGzipBytes: median.cssGzipBytes,
};

console.log(
  JSON.stringify(
    {
      medianWallMs: report.medianWallMs,
      cssRawBytes: report.cssRawBytes,
      cssGzipBytes: report.cssGzipBytes,
    },
    null,
    2,
  ),
);

if (outPath) {
  const absolute = resolve(root, outPath);
  mkdirSync(join(absolute, '..'), { recursive: true });
  writeFileSync(absolute, `${JSON.stringify(report, null, 2)}\n`);
  console.log(`wrote ${outPath}`);
}
