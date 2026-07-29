#!/usr/bin/env bun
/**
 * Forbid value-based `switch (` statements in src/.
 * Generated bindings.ts is excluded.
 */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';

const root = process.cwd();

/**
 * @param {string} source
 * @param {string} relativePath
 * @returns {string[]}
 */
export function findValueSwitches(source, relativePath) {
  /** @type {string[]} */
  const hits = [];
  const lines = source.split('\n');
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    if (/\bswitch\s*\(/.test(line)) {
      hits.push(`${relativePath}:${index + 1}: ${line.trim()}`);
    }
  }
  return hits;
}

/**
 * @param {string} dir
 * @returns {string[]}
 */
function walk(dir) {
  /** @type {string[]} */
  const files = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) files.push(...walk(full));
    else if (/\.(?:ts|tsx)$/.test(entry.name)) files.push(full);
  }
  return files;
}

/**
 * @param {string} file
 * @param {string} base
 */
function relTo(file, base) {
  return relative(base, file).replaceAll('\\', '/');
}

if (import.meta.main) {
  const targetArg = process.argv[2];
  const scanRoot = targetArg ? join(root, targetArg) : join(root, 'src');
  const scanStat = statSync(scanRoot);
  const files = scanStat.isDirectory() ? walk(scanRoot) : [scanRoot];

  /** @type {string[]} */
  const offenders = [];
  for (const file of files) {
    const relativePath = relTo(file, scanRoot);
    if (relativePath === 'bindings.ts') {
      continue;
    }
    const source = readFileSync(file, 'utf8');
    offenders.push(...findValueSwitches(source, relativePath));
  }

  if (offenders.length > 0) {
    console.error('Value-based switch statements found:\n');
    for (const offender of offenders) console.error(`  - ${offender}`);
    process.exit(1);
  }

  console.log('No value-based switch statements found.');
}
