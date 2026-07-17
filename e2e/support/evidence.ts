import { copyFile, mkdir, readFile, readdir, rename, rm, stat, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { Effect } from 'effect';

import { FIXTURE_PASSWORD } from '../app/fixture-registry';
import { FAILURES_ROOT, OWNERSHIP_FILE, OWNERSHIP_MARKER } from '../constants';
import { E2eEvidenceError } from './errors';
import type { BuildManifest } from './fingerprint';
import { ensureOwnedDirectory, removeOwnedDirectory } from './ownership';

const SAFE_EXTENSIONS = new Set(['.json', '.log', '.png', '.txt']);
const UNSAFE_PATTERNS = [
  /["']?(?:password|access[_-]?token|authorization)["']?\s*[:=]\s*\S+/i,
  /https?:\/\/[^/\s:@]+:[^@\s]+@/i,
];

export function redactEvidenceText(value: string): string {
  return value
    .split(FIXTURE_PASSWORD)
    .join('[REDACTED]')
    .replaceAll(
      /["']?(?:password|access[_-]?token|authorization)["']?\s*[:=]\s*(?:"[^"]*"|'[^']*'|[^,\s}\]]+)/gi,
      '[REDACTED CREDENTIAL]',
    )
    .replaceAll(/https?:\/\/[^/\s:@]+:[^@\s]+@/gi, 'https://[REDACTED]@');
}

export function evidenceTextIsSafe(value: string): boolean {
  return (
    !value.includes(FIXTURE_PASSWORD) && UNSAFE_PATTERNS.every((pattern) => !pattern.test(value))
  );
}

async function collectSafeFiles(source: string): Promise<string[]> {
  const entries = await readdir(source, { withFileTypes: true }).catch(() => []);
  const files = await Promise.all(
    entries.map(async (entry) => {
      const target = path.join(source, entry.name);
      if (entry.isDirectory()) return collectSafeFiles(target);
      if (!entry.isFile() || !isRetainableEvidenceFile(entry.name)) return [];
      return [target];
    }),
  );
  return files.flat();
}

export function isRetainableEvidenceFile(name: string): boolean {
  return (
    SAFE_EXTENSIONS.has(path.extname(name)) &&
    !name.includes('page-source') &&
    !name.includes('environment')
  );
}

async function pruneFailures(): Promise<void> {
  const entries = await readdir(FAILURES_ROOT, { withFileTypes: true });
  const owned = await Promise.all(
    entries
      .filter((entry) => entry.isDirectory())
      .map(async (entry) => {
        const directory = path.join(FAILURES_ROOT, entry.name);
        const marker = await readFile(path.join(directory, OWNERSHIP_FILE), 'utf8').catch(() => '');
        if (marker.trim() !== OWNERSHIP_MARKER) return null;
        const metadata = await stat(directory);
        return { directory, modified: metadata.mtimeMs };
      }),
  );
  const stale = selectStaleFailures(
    owned.filter((entry): entry is NonNullable<typeof entry> => entry !== null),
  );
  for (const entry of stale) await rm(entry.directory, { force: true, recursive: true });
}

export function selectStaleFailures<T extends { readonly modified: number }>(
  entries: readonly T[],
): T[] {
  return entries.toSorted((left, right) => right.modified - left.modified).slice(5);
}

export const retainFailureEvidence = Effect.fn('e2e.retainFailureEvidence')(function* (
  sandbox: string,
  spec: string,
  failure: unknown,
  manifest: BuildManifest,
) {
  yield* ensureOwnedDirectory(FAILURES_ROOT);
  return yield* Effect.tryPromise({
    try: async () => {
      const stage = path.join(sandbox, '.evidence-stage');
      await mkdir(stage, { recursive: true });
      await writeFile(path.join(stage, OWNERSHIP_FILE), `${OWNERSHIP_MARKER}\n`);
      await writeFile(
        path.join(stage, 'summary.json'),
        `${JSON.stringify(
          {
            spec,
            failure: failure instanceof Error ? failure.message : String(failure),
            retainedAt: new Date().toISOString(),
          },
          null,
          2,
        )}\n`,
      );
      await writeFile(
        path.join(stage, 'tool-versions.json'),
        `${JSON.stringify(manifest.toolVersions, null, 2)}\n`,
      );
      await writeFile(
        path.join(stage, 'build-fingerprint.json'),
        `${JSON.stringify(
          { appFingerprint: manifest.appFingerprint, binaryHash: manifest.binaryHash },
          null,
          2,
        )}\n`,
      );

      for (const source of await collectSafeFiles(path.join(sandbox, 'logs'))) {
        await copyFile(source, path.join(stage, path.basename(source)));
      }

      for (const entry of await readdir(stage, { withFileTypes: true })) {
        if (
          !entry.isFile() ||
          path.extname(entry.name) === '.png' ||
          entry.name === OWNERSHIP_FILE
        ) {
          continue;
        }
        const filePath = path.join(stage, entry.name);
        const redacted = redactEvidenceText(await readFile(filePath, 'utf8'));
        await writeFile(filePath, redacted);
        if (!evidenceTextIsSafe(redacted)) {
          await rm(stage, { force: true, recursive: true });
          throw new Error('Unsafe credential-shaped material remained after evidence redaction.');
        }
      }

      const identifier = `${new Date().toISOString().replaceAll(':', '-')}-${path.basename(spec, '.e2e.ts')}`;
      const destination = path.join(FAILURES_ROOT, identifier);
      await rename(stage, destination);
      await pruneFailures();
      return destination;
    },
    catch: (cause) =>
      new E2eEvidenceError({
        message: cause instanceof Error ? cause.message : String(cause),
      }),
  });
});

export const discardEvidenceDirectory = removeOwnedDirectory;
