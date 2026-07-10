import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const distRoot = fileURLToPath(new URL('../dist/', import.meta.url));

async function filesUnder(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = await Promise.all(
    entries.map((entry) => {
      const path = join(directory, entry.name);
      return entry.isDirectory() ? filesUnder(path) : [path];
    }),
  );
  return files.flat();
}

const files = await filesUnder(distRoot);
const cssFiles = files.filter((file) => file.endsWith('.css'));

if (cssFiles.length === 0) {
  throw new Error('Project Theme check could not find emitted CSS in dist/');
}

const cssContents = await Promise.all(cssFiles.map((file) => readFile(file, 'utf8')));
const css = cssContents.join('\n');
const compactCss = css.replaceAll(/\s+/g, '');

const requiredFragments = [
  ':root[data-theme=light]',
  ':root[data-theme=dark]',
  ':root[data-theme-id=neutral][data-theme=light]',
  ':root[data-theme-id=jellypilot][data-theme=dark]',
  '--jellypilot-color-background:#f7f8fc',
  '--jellypilot-color-background:#05060a',
  '--colors-background:#fff',
  '--colors-background:#05060a',
  '--radii-sm:.375rem',
  '--radii-md:.5rem',
  '--radii-lg:.75rem',
  '--jellypilot-font-sans:"FigtreeVariable",ui-sans-serif,system-ui,sans-serif',
  '@font-face{font-family:FigtreeVariable',
];

for (const fragment of requiredFragments) {
  if (!compactCss.includes(fragment)) {
    throw new Error(`Project Theme build output is missing ${fragment}`);
  }
}

console.log('Project Theme build output contains light, dark, and Figtree contracts');
