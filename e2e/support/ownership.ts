import { access, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { Effect } from 'effect';

import { OWNERSHIP_FILE, OWNERSHIP_MARKER } from '../constants';
import { E2eOwnershipError } from './errors';

async function exists(target: string): Promise<boolean> {
  try {
    await access(target);
    return true;
  } catch (error) {
    if (typeof error === 'object' && error !== null && 'code' in error && error.code === 'ENOENT') {
      return false;
    }
    throw error;
  }
}

export const ensureOwnedDirectory = (directory: string) =>
  Effect.tryPromise({
    try: async () => {
      const markerPath = path.join(directory, OWNERSHIP_FILE);
      if (await exists(directory)) {
        const marker = await readFile(markerPath, 'utf8');
        if (marker.trim() !== OWNERSHIP_MARKER) {
          throw new Error(`Refusing to claim an unowned E2E directory: ${directory}`);
        }
        return directory;
      }

      await mkdir(directory, { recursive: true });
      await writeFile(markerPath, `${OWNERSHIP_MARKER}\n`, { flag: 'wx' });
      return directory;
    },
    catch: (cause) =>
      new E2eOwnershipError({
        path: directory,
        message: cause instanceof Error ? cause.message : String(cause),
      }),
  });

export const removeOwnedDirectory = (directory: string) =>
  Effect.tryPromise({
    try: async () => {
      if (!(await exists(directory))) return;
      const marker = await readFile(path.join(directory, OWNERSHIP_FILE), 'utf8');
      if (marker.trim() !== OWNERSHIP_MARKER) {
        throw new Error(`Refusing to remove an unowned E2E directory: ${directory}`);
      }
      await rm(directory, { force: true, recursive: true });
    },
    catch: (cause) =>
      new E2eOwnershipError({
        path: directory,
        message: cause instanceof Error ? cause.message : String(cause),
      }),
  });

export function withOwnedDirectory<A, E, R>(
  directory: string,
  effect: Effect.Effect<A, E, R>,
): Effect.Effect<A, E | E2eOwnershipError, R> {
  return Effect.scoped(
    Effect.acquireRelease(ensureOwnedDirectory(directory), () =>
      removeOwnedDirectory(directory).pipe(
        Effect.catch((error) =>
          Effect.logError('Could not release an owned E2E directory.', error),
        ),
      ),
    ).pipe(Effect.flatMap(() => effect)),
  );
}
