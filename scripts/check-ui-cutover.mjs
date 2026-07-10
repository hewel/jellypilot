import { readdir, readFile, stat } from 'node:fs/promises';
import { join, relative } from 'node:path';

const roots = ['src', 'tests'];
const forbidden = [
  { pattern: /@ark-ui\//, label: 'Ark UI dependency' },
  { pattern: /@vanilla-extract\/sprinkles/, label: 'Sprinkles dependency' },
  { pattern: /src\/components\/ui\b/, label: 'legacy local primitive path' },
  { pattern: /(?:\.{1,2}\/)*components\/ui\b/, label: 'legacy local primitive import' },
  { pattern: /src\/styles\/sprinkles\.css/, label: 'legacy Sprinkles owner' },
  { pattern: /(?:\.{1,2}\/)*styles\/sprinkles\.css/, label: 'legacy Sprinkles import' },
  { pattern: /src\/styles\/vars\.css/, label: 'legacy app theme owner' },
  { pattern: /(?:\.{1,2}\/)*styles\/vars\.css/, label: 'legacy app theme import' },
];

async function filesUnder(root) {
  const entries = await readdir(root, { withFileTypes: true });
  const files = await Promise.all(
    entries.map(async (entry) => {
      const path = join(root, entry.name);
      if (entry.isDirectory()) return filesUnder(path);
      return entry.isFile() ? [path] : [];
    }),
  );
  return files.flat();
}

const filesByRoot = await Promise.all(roots.map(filesUnder));
const files = filesByRoot.flat();
const violations = [];

for (const file of files) {
  const source = await readFile(file, 'utf8');
  for (const { pattern, label } of forbidden) {
    if (pattern.test(source)) {
      violations.push(`${relative(process.cwd(), file)}: ${label}`);
    }
  }
}

for (const file of ['package.json']) {
  const source = await readFile(file, 'utf8');
  for (const { pattern, label } of forbidden.slice(0, 2)) {
    if (pattern.test(source)) {
      violations.push(`${file}: ${label}`);
    }
  }
}

for (const path of ['src/components/ui', 'src/styles/sprinkles.css.ts', 'src/styles/vars.css.ts']) {
  try {
    await stat(path);
    violations.push(`${path}: legacy owner still exists`);
  } catch (error) {
    if (error?.code !== 'ENOENT') throw error;
  }
}

if (violations.length > 0) {
  throw new Error(`UI cutover guard failed:\n${violations.join('\n')}`);
}

console.log('UI cutover guard passed');
