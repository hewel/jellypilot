#!/usr/bin/env bun
/**
 * Styling-boundary checker for Panda CSS.
 * Rejects:
 *  - raw palette token paths outside the theme
 *  - cross-component private-style imports
 *  - legacy vanilla-extract imports and `.css.ts` modules
 */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';

const root = process.cwd();
const srcRoot = join(root, 'src');
const themeFiles = new Set(['src/styles/theme-tokens.ts', 'panda.config.ts']);

const RAW_PALETTE =
  /\b(?:colors\.)?(?:neutral|indigo|teal|amber|red|cyan)\.(?:0|50|300|400|500|600|700|750|800|850|900|925|950|975|1000)\b/g;
const VE_IMPORT = /from\s+['"]@vanilla-extract\/[^'"]+['"]/;
const PRIVATE_STYLE_IMPORT = /from\s+['"]((?:\.\.?\/)+[^'"]+\.styles(?:\.ts)?)['"]/g;

const errors: string[] = [];

function walk(dir: string): string[] {
  const files: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) files.push(...walk(full));
    else if (/\.(?:[cm]?[jt]sx?|css\.ts|styles\.ts)$/.test(entry.name)) files.push(full);
  }
  return files;
}

function rel(file: string): string {
  return relative(root, file).replaceAll('\\', '/');
}

function isThemeDefinition(fileRel: string): boolean {
  return themeFiles.has(fileRel) || fileRel.endsWith('/theme-tokens.ts');
}

function checkRawPalette(fileRel: string, source: string): void {
  if (isThemeDefinition(fileRel)) return;
  const matches = source.match(RAW_PALETTE);
  if (!matches) return;
  for (const match of matches) {
    errors.push(`${fileRel}: raw palette token '${match}' is only allowed in theme definitions`);
  }
}

function checkEngineImports(fileRel: string, source: string): void {
  if (VE_IMPORT.test(source)) {
    errors.push(`${fileRel}: vanilla-extract imports are forbidden; use @styled-system`);
  }
  if (fileRel.endsWith('.css.ts')) {
    errors.push(`${fileRel}: legacy .css.ts modules are forbidden; use .styles.ts`);
  }
}

function checkPrivateStyleImports(fileRel: string, source: string): void {
  if (!fileRel.startsWith('src/')) return;
  const fileBase =
    fileRel
      .split('/')
      .pop()
      ?.replace(/\.(?:tsx?|jsx?)$/, '') ?? '';

  for (const match of source.matchAll(PRIVATE_STYLE_IMPORT)) {
    const spec = match[1];
    if (!spec) continue;

    // Shared style infrastructure remains public.
    if (spec.includes('/styles/') || /(^|\/)styles\//.test(spec)) continue;

    // Adjacent owner style module: Button.tsx -> ./Button.css or ./Button.styles
    if (spec.startsWith('./') && !spec.slice(2).includes('/')) {
      const importedBase = spec.replace(/^\.\//, '').replace(/\.(?:css|styles)(?:\.ts)?$/, '');
      if (importedBase === fileBase) continue;
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

function existsAsFile(path: string): boolean {
  try {
    return statSync(path).isFile();
  } catch {
    return false;
  }
}

const LAYOUT_PROPS = new Set([
  'width',
  'height',
  'min-width',
  'min-height',
  'max-width',
  'max-height',
  'top',
  'right',
  'bottom',
  'left',
  'inset',
  'margin',
  'margin-top',
  'margin-right',
  'margin-bottom',
  'margin-left',
  'padding',
  'padding-top',
  'padding-right',
  'padding-bottom',
  'padding-left',
  'gap',
  'row-gap',
  'column-gap',
  'border-width',
  'flex-basis',
]);
const BREAKPOINT_KEYS = new Set(['sm', 'md', 'lg', 'xl', '2xl']);
const PSEUDO_KEYS = new Set(['_hover', '_active', '_focus', '_focusVisible', '_focusWithin']);
const MOTION_BLOCK_RE = /export const (\w+) = (?:css|cva)\(\{/g;

function skipString(source: string, start: number): number {
  const quote = source[start];
  let i = start + 1;
  const n = source.length;
  while (i < n) {
    const c = source[i];
    if (c === '\\') {
      i += 2;
      continue;
    }
    if (quote === '`' && c === '$' && source[i + 1] === '{') {
      const close = matchBrace(source, i + 1);
      i = close === -1 ? n : close + 1;
      continue;
    }
    if (c === quote) return i + 1;
    i += 1;
  }
  return n;
}

function matchBrace(source: string, openIdx: number): number {
  let depth = 0;
  let i = openIdx;
  const n = source.length;
  while (i < n) {
    const c = source[i];
    if (c === '/' && source[i + 1] === '/') {
      const nl = source.indexOf('\n', i);
      i = nl === -1 ? n : nl + 1;
      continue;
    }
    if (c === '/' && source[i + 1] === '*') {
      const end = source.indexOf('*/', i + 2);
      i = end === -1 ? n : end + 2;
      continue;
    }
    if (c === "'" || c === '"' || c === '`') {
      i = skipString(source, i);
      continue;
    }
    if (c === '{') depth += 1;
    else if (c === '}') {
      depth -= 1;
      if (depth === 0) return i;
    }
    i += 1;
  }
  return -1;
}

function skipValue(body: string, start: number): number {
  let i = start;
  const n = body.length;
  let paren = 0;
  let bracket = 0;
  let brace = 0;
  while (i < n) {
    const c = body[i];
    if (c === '/' && body[i + 1] === '/') {
      const nl = body.indexOf('\n', i);
      i = nl === -1 ? n : nl + 1;
      continue;
    }
    if (c === '/' && body[i + 1] === '*') {
      const end = body.indexOf('*/', i + 2);
      i = end === -1 ? n : end + 2;
      continue;
    }
    if (c === "'" || c === '"' || c === '`') {
      i = skipString(body, i);
      continue;
    }
    if (c === '(') paren += 1;
    else if (c === ')') {
      if (paren === 0) break;
      paren -= 1;
    } else if (c === '[') bracket += 1;
    else if (c === ']') {
      if (bracket === 0) break;
      bracket -= 1;
    } else if (c === '{') brace += 1;
    else if (c === '}') {
      if (brace === 0) break;
      brace -= 1;
    } else if (c === ',' && paren === 0 && bracket === 0 && brace === 0) break;
    i += 1;
  }
  return i;
}

interface StyleProperty {
  key: string;
  path: string[];
  value: string;
}

function scanObject(body: string, path: string[], acc: StyleProperty[]): void {
  let i = 0;
  const n = body.length;
  while (i < n) {
    const c = body[i];
    if (/\s/.test(c)) {
      i += 1;
      continue;
    }
    if (c === '/' && body[i + 1] === '/') {
      const nl = body.indexOf('\n', i);
      i = nl === -1 ? n : nl + 1;
      continue;
    }
    if (c === '/' && body[i + 1] === '*') {
      const end = body.indexOf('*/', i + 2);
      i = end === -1 ? n : end + 2;
      continue;
    }
    let key: string;
    if (c === "'" || c === '"' || c === '`') {
      const end = skipString(body, i);
      key = body.slice(i + 1, end - 1);
      i = end;
    } else if (/[A-Za-z0-9_$]/.test(c)) {
      let j = i;
      while (j < n && /[A-Za-z0-9_$]/.test(body[j])) j += 1;
      key = body.slice(i, j);
      i = j;
    } else {
      i += 1;
      continue;
    }
    while (i < n && /\s/.test(body[i])) i += 1;
    if (body[i] !== ':') {
      i = skipValue(body, i);
      continue;
    }
    i += 1;
    while (i < n && /\s/.test(body[i])) i += 1;
    if (body[i] === '{') {
      const close = matchBrace(body, i);
      if (close === -1) return;
      scanObject(body.slice(i + 1, close), [...path, key], acc);
      i = close + 1;
      continue;
    }
    const valueStart = i;
    i = skipValue(body, i);
    const value = body.slice(valueStart, i).trim();
    if (key === 'transform' || key === 'transitionProperty') {
      acc.push({ key, path: [...path], value });
    }
  }
}

function isConditionalKey(key: string): boolean {
  if (key.startsWith('_')) return true;
  if (key === 'variants' || key === 'compoundVariants') return true;
  if (BREAKPOINT_KEYS.has(key)) return true;
  if (key.startsWith('&') || key.startsWith('[') || key.startsWith('@')) return true;
  if (key.includes(':hover') || key.includes(':active') || key.includes(':focus')) return true;
  return false;
}

function isInteractivePath(path: string[]): boolean {
  return path.some(
    (key) =>
      PSEUDO_KEYS.has(key) ||
      key.includes(':hover') ||
      key.includes(':active') ||
      key.includes(':focus'),
  );
}

function dataStateValue(key: string): string | null {
  const match = key.match(/data-state=(?:"([^"]*)"|'([^']*)'|([^\]"'\s,]+))/);
  if (!match) return null;
  return match[1] ?? match[2] ?? match[3] ?? null;
}

function parseTransitionItems(value: string): string[] {
  return value
    .replaceAll(/[[\]'"]/g, '')
    .split(',')
    .map((item) => item.trim())
    .filter(Boolean);
}

export function collectMotionInvariantErrors(fileRel: string, source: string): string[] {
  const result: string[] = [];
  for (const match of source.matchAll(MOTION_BLOCK_RE)) {
    const name = match[1];
    const openIdx = match.index + match[0].length - 1;
    const closeIdx = matchBrace(source, openIdx);
    if (closeIdx === -1) continue;
    const acc: StyleProperty[] = [];
    scanObject(source.slice(openIdx + 1, closeIdx), [], acc);

    const transforms = acc.filter((entry) => entry.key === 'transform');
    const transitions = acc.filter((entry) => entry.key === 'transitionProperty');

    for (const transition of transitions) {
      for (const item of parseTransitionItems(transition.value)) {
        if (LAYOUT_PROPS.has(item)) {
          result.push(
            `${fileRel}: layout property '${item}' must not appear in transitionProperty (${name})`,
          );
        }
      }
    }

    const hasResting = transforms.some((entry) => !entry.path.some(isConditionalKey));
    const baseTransitionHasTransform = transitions.some(
      (entry) =>
        !entry.path.some(isConditionalKey) &&
        parseTransitionItems(entry.value).includes('transform'),
    );
    if (hasResting || !baseTransitionHasTransform) continue;

    if (transforms.some((entry) => isInteractivePath(entry.path))) {
      result.push(`${fileRel}: interactive-state 'transform' has no resting value (${name})`);
      continue;
    }

    const dataStateValues = new Set<string>();
    for (const entry of transforms) {
      for (const key of entry.path) {
        const value = dataStateValue(key);
        if (value !== null) dataStateValues.add(value);
      }
    }
    if (dataStateValues.size === 1) {
      result.push(`${fileRel}: interactive-state 'transform' has no resting value (${name})`);
    }
  }
  return result;
}

function main(): void {
  const files = [...walk(srcRoot), join(root, 'panda.config.ts')].filter((f) => existsAsFile(f));

  for (const file of files) {
    const fileRel = rel(file);
    const source = readFileSync(file, 'utf8');
    checkRawPalette(fileRel, source);
    checkEngineImports(fileRel, source);
    checkPrivateStyleImports(fileRel, source);
    errors.push(...collectMotionInvariantErrors(fileRel, source));
  }

  if (errors.length > 0) {
    console.error('Styling boundary check failed:\n');
    for (const error of errors) console.error(`  - ${error}`);
    process.exit(1);
  }

  console.log('Styling boundary check passed.');
}

if (import.meta.main) main();
