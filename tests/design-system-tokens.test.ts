import * as fs from 'fs';
import * as path from 'path';

import { expect, test } from '@rstest/core';

test('design-token contract exposes semantic theme groups and dark values', () => {
  const varsSource = fs.readFileSync(path.resolve(__dirname, '../src/styles/vars.css.ts'), 'utf8');

  for (const group of [
    'background',
    'text',
    'icon',
    'border',
    'accent',
    'status',
    'data',
    'space',
    'size',
    'radius',
    'shadow',
    'motion',
    'font',
    'typeScale',
  ]) {
    expect(varsSource).toContain(`${group}:`);
  }
  expect(varsSource).toContain("createGlobalTheme(':root'");
  expect(varsSource).toContain('createGlobalTheme("[data-theme=\'dark\']"');
});

test('Figtree is the only bundled variable UI font', () => {
  const packageJson = fs.readFileSync(path.resolve(__dirname, '../package.json'), 'utf8');
  const indexSource = fs.readFileSync(path.resolve(__dirname, '../src/index.tsx'), 'utf8');

  expect(packageJson).toContain('"@fontsource-variable/figtree"');
  expect(packageJson).not.toContain('"@fontsource-variable/inter"');
  expect(packageJson).not.toContain('"@fontsource-variable/space-grotesk"');
  expect(indexSource).toContain("import '@fontsource-variable/figtree';");
});
