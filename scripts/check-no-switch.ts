#!/usr/bin/env bun
/**
 * Forbid value-based `switch (` statements in src/.
 * Generated bindings.ts is excluded.
 */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';

const root = process.cwd();

export function findValueSwitches(source: string, relativePath: string): string[] {
  const hits: string[] = [];
  const lines = source.split('\n');
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    if (/\bswitch\s*\(/.test(line)) {
      hits.push(`${relativePath}:${index + 1}: ${line.trim()}`);
    }
  }
  return hits;
}

function walk(dir: string): string[] {
  const files: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) files.push(...walk(full));
    else if (/\.(?:ts|tsx)$/.test(entry.name)) files.push(full);
  }
  return files;
}

function relTo(file: string, base: string): string {
  return relative(base, file).replaceAll('\\', '/');
}

if (import.meta.main) {
  const targetArg = process.argv[2];
  const scanRoot = targetArg ? join(root, targetArg) : join(root, 'src');
  const scanStat = statSync(scanRoot);
  const files = scanStat.isDirectory() ? walk(scanRoot) : [scanRoot];

  const offenders: string[] = [];
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
