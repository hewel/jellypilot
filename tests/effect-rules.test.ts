import * as fs from 'fs';
import * as path from 'path';

import { expect, test } from '@rstest/core';

function listSourceFiles(dir: string, acc: string[] = []): string[] {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      listSourceFiles(fullPath, acc);
    } else if (entry.isFile() && (entry.name.endsWith('.ts') || entry.name.endsWith('.tsx'))) {
      acc.push(fullPath);
    }
  }
  return acc;
}

test('no value-based switch statements in src (generated bindings excluded)', () => {
  const srcRoot = path.resolve(__dirname, '../src');
  const bindingsPath = path.join(srcRoot, 'bindings.ts');
  const offenders: string[] = [];

  for (const file of listSourceFiles(srcRoot)) {
    if (file === bindingsPath) {
      continue;
    }
    const lines = fs.readFileSync(file, 'utf8').split('\n');
    lines.forEach((line, index) => {
      if (/\bswitch\s*\(/.test(line)) {
        offenders.push(`${path.relative(srcRoot, file)}:${index + 1}: ${line.trim()}`);
      }
    });
  }

  expect(offenders).toEqual([]);
});
