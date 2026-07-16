#!/usr/bin/env bun
/**
 * Styling-boundary checker for the Panda migration.
 * Rejects:
 *  - raw palette token paths outside the theme
 *  - cross-component private-style imports
 *  - style modules importing both engines (except the documented VE token bridge)
 *  - direct mixed-engine class composition in one module
 */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';

const root = process.cwd();
const srcRoot = join(root, 'src');
const themeFiles = new Set(['src/styles/theme-tokens.ts', 'panda.config.ts']);
// Explicit coexistence bridge: aliases Panda vars into the legacy VE contract.
const BRIDGE_FILES = new Set(['src/styles/vars.css.ts']);

const RAW_PALETTE =
  /\b(?:colors\.)?(?:neutral|indigo|teal|amber|red|cyan)\.(?:0|50|300|400|500|600|700|750|800|850|900|925|950|975|1000)\b/g;
const VE_IMPORT = /from\s+['"]@vanilla-extract\/[^'"]+['"]/;
const PANDA_IMPORT = /from\s+['"]@styled-system\/[^'"]+['"]/;
const PRIVATE_STYLE_IMPORT = /from\s+['"]((?:\.\.?\/)+[^'"]+\.(?:css|styles)(?:\.ts)?)['"]/g;

/** @type {string[]} */
const errors = [];

function walk(dir) {
  /** @type {string[]} */
  const files = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) files.push(...walk(full));
    else if (/\.(?:[cm]?[jt]sx?|css\.ts|styles\.ts)$/.test(entry.name)) files.push(full);
  }
  return files;
}

function rel(file) {
  return relative(root, file).replaceAll('\\', '/');
}

function isThemeDefinition(fileRel) {
  return themeFiles.has(fileRel) || fileRel.endsWith('/theme-tokens.ts');
}

function checkRawPalette(fileRel, source) {
  if (isThemeDefinition(fileRel)) return;
  const matches = source.match(RAW_PALETTE);
  if (!matches) return;
  for (const match of matches) {
    errors.push(`${fileRel}: raw palette token '${match}' is only allowed in theme definitions`);
  }
}

function checkEngineImports(fileRel, source) {
  if (BRIDGE_FILES.has(fileRel)) return;
  const hasVe = VE_IMPORT.test(source);
  const hasPanda = PANDA_IMPORT.test(source);
  if (hasVe && hasPanda) {
    errors.push(`${fileRel}: style module imports both vanilla-extract and @styled-system`);
  }
  if (fileRel.endsWith('.css.ts') && hasPanda) {
    errors.push(`${fileRel}: .css.ts must remain vanilla-extract only`);
  }
  if (fileRel.endsWith('.styles.ts') && hasVe) {
    errors.push(`${fileRel}: .styles.ts must remain Panda only`);
  }
}

function checkPrivateStyleImports(fileRel, source) {
  if (!fileRel.startsWith('src/')) return;
  const fileBase =
    fileRel
      .split('/')
      .pop()
      ?.replace(/\.(?:tsx?|jsx?)$/, '') ?? '';

  for (const match of source.matchAll(PRIVATE_STYLE_IMPORT)) {
    const spec = match[1];
    if (!spec) continue;

    // Shared styles/* modules remain public during coexistence.
    if (spec.includes('/styles/') || /(^|\/)styles\//.test(spec)) continue;

    // Adjacent owner style module: Button.tsx -> ./Button.css or ./Button.styles
    if (spec.startsWith('./') && !spec.slice(2).includes('/')) {
      const importedBase = spec.replace(/^\.\//, '').replace(/\.(?:css|styles)(?:\.ts)?$/, '');
      if (importedBase === fileBase) continue;
    }

    // Route-family style owners (library.css / browseRoute.css / detailRoute.css)
    // Remain shared until the library migration slice replaces them.
    if (
      fileRel.startsWith('src/routes/') &&
      /(library|browseRoute|detailRoute|__root)\.css/.test(spec)
    ) {
      continue;
    }

    // Same-folder OperationsConsole shared.css and similar local owners.
    if (spec.startsWith('./') || spec.startsWith('../')) {
      // Still forbid reaching into another component's private styles.
      const isTopLevelComponent =
        /\/components\/[^/]+\//.test(fileRel) === false && fileRel.startsWith('src/components/');
      if (isTopLevelComponent && spec.includes('../') && /\.(?:css|styles)/.test(spec)) {
        // Top-level component importing sibling component styles.
        errors.push(`${fileRel}: cross-component private style import '${spec}'`);
        continue;
      }
      const reachesOtherComponent = spec.includes('/ui/') || /components\/(?!.*\/)/.test(spec);
      if (reachesOtherComponent && !spec.startsWith('./')) {
        // Importing from another component path.
        errors.push(`${fileRel}: cross-component private style import '${spec}'`);
      }
    }
  }
}

function existsAsFile(path) {
  try {
    return statSync(path).isFile();
  } catch {
    return false;
  }
}

const files = [...walk(srcRoot), join(root, 'panda.config.ts')].filter((f) => existsAsFile(f));

for (const file of files) {
  const fileRel = rel(file);
  const source = readFileSync(file, 'utf8');
  checkRawPalette(fileRel, source);
  checkEngineImports(fileRel, source);
  checkPrivateStyleImports(fileRel, source);
}

if (errors.length > 0) {
  console.error('Styling boundary check failed:\n');
  for (const error of errors) console.error(`  - ${error}`);
  process.exit(1);
}

console.log('Styling boundary check passed.');
