import { lstat, mkdir, mkdtemp, readFile, readlink, rename, rm, symlink } from 'node:fs/promises';
import path from 'node:path';

import { Data, Effect, Option } from 'effect';

import { command } from './commands';
import { REPO_ROOT } from './constants';
import { runCommand } from './process';

const VENDOR = path.join(REPO_ROOT, 'target/vendor');

class TaskIcedSourceError extends Data.TaggedError('TaskIcedSourceError')<{
  readonly message: string;
  readonly cause?: unknown;
}> {}

const sourceIo = <A>(operation: () => Promise<A>) =>
  Effect.tryPromise({
    try: operation,
    catch: (cause) =>
      new TaskIcedSourceError({
        message: `iced source preparation failed: ${String(cause)}`,
        cause,
      }),
  });

const pathStatus = (location: string) =>
  sourceIo(() => lstat(location)).pipe(
    Effect.map(Option.some),
    Effect.catchTag('TaskIcedSourceError', (error) =>
      typeof error.cause === 'object' &&
      error.cause !== null &&
      'code' in error.cause &&
      error.cause.code === 'ENOENT'
        ? Effect.succeed(Option.none())
        : Effect.fail(error),
    ),
  );

const verifyCheckout = Effect.fn('task.iced.verifyCheckout')(function* (
  checkout: string,
  revision: string,
) {
  for (const directory of [checkout, path.join(checkout, '.git')]) {
    const status = yield* pathStatus(directory);
    if (Option.isNone(status) || !status.value.isDirectory() || status.value.isSymbolicLink()) {
      return yield* Effect.fail(
        new TaskIcedSourceError({ message: `Refusing non-owned iced checkout at ${directory}.` }),
      );
    }
  }
  const head = yield* runCommand({
    ...command('git', ['-C', checkout, 'rev-parse', 'HEAD']),
    buffered: true,
  });
  const dirty = yield* runCommand({
    ...command('git', ['-C', checkout, 'status', '--porcelain', '--untracked-files=no']),
    buffered: true,
  });
  if (head.stdout.trim() !== revision || dirty.stdout.trim() !== '') {
    return yield* Effect.fail(
      new TaskIcedSourceError({
        message: `Prepared iced source at ${checkout} must have HEAD ${revision} and no tracked edits. Preserve any local work and move this checkout aside before preparing again.`,
      }),
    );
  }
});

const verifyStableLink = Effect.fn('task.iced.verifyStableLink')(function* (link: string) {
  const status = yield* pathStatus(link);
  if (Option.isNone(status)) return;
  if (!status.value.isSymbolicLink()) {
    return yield* Effect.fail(
      new TaskIcedSourceError({
        message: `Refusing to replace non-symlink ${link}. Move it aside without deleting its contents.`,
      }),
    );
  }
  const destination = path.resolve(VENDOR, yield* sourceIo(() => readlink(link)));
  // Older generated checkouts used a longer digest in their directory name.
  // Only switch links within our owned namespace; never remove their contents.
  if (
    path.dirname(destination) !== VENDOR ||
    !/^iced-(?:[a-f0-9]{40}|[a-f0-9]{64})$/.test(path.basename(destination))
  ) {
    return yield* Effect.fail(
      new TaskIcedSourceError({
        message: `Refusing to take over ${link}: it does not point to an owned vendor checkout.`,
      }),
    );
  }
  const target = yield* pathStatus(destination);
  if (Option.isSome(target) && (!target.value.isDirectory() || target.value.isSymbolicLink())) {
    return yield* Effect.fail(
      new TaskIcedSourceError({
        message: `Refusing indirect or non-directory iced link target ${destination}.`,
      }),
    );
  }
});

export const prepareIced = Effect.fn('task.iced.prepare')(function* (source: string | null) {
  const pinPath = path.join(REPO_ROOT, 'tools/embedded-mpv/iced-source.json');
  const pin: unknown = yield* sourceIo(async () => JSON.parse(await readFile(pinPath, 'utf8')));
  if (
    typeof pin !== 'object' ||
    pin === null ||
    !('revision' in pin) ||
    typeof pin.revision !== 'string' ||
    !/^[a-f0-9]{40}$/.test(pin.revision) ||
    !('remote' in pin) ||
    typeof pin.remote !== 'string' ||
    !/^https:\/\/[A-Za-z0-9.-]+\/[A-Za-z0-9._/-]+\.git$/.test(pin.remote) ||
    Object.keys(pin).some((key) => key !== 'remote' && key !== 'revision')
  ) {
    return yield* Effect.fail(
      new TaskIcedSourceError({
        message: `Invalid ${pinPath}: expected only an HTTPS remote and a full lowercase commit SHA revision.`,
      }),
    );
  }
  const { remote, revision } = pin;
  // Check each parent before creating descendants, rather than following an
  // accidental target/vendor symlink into somebody else's source directory.
  for (const directory of [path.join(REPO_ROOT, 'target'), VENDOR]) {
    const status = yield* pathStatus(directory);
    if (Option.isNone(status)) {
      yield* sourceIo(() => mkdir(directory));
    } else if (!status.value.isDirectory() || status.value.isSymbolicLink()) {
      return yield* Effect.fail(
        new TaskIcedSourceError({
          message: `Refusing non-directory or symlink preparation root ${directory}.`,
        }),
      );
    }
  }
  const checkout = path.join(VENDOR, `iced-${revision}`);
  const link = path.join(VENDOR, 'iced');
  yield* verifyStableLink(link);
  if (Option.isNone(yield* pathStatus(checkout))) {
    const origin = source === null ? remote : path.resolve(REPO_ROOT, source);
    if (source !== null) {
      const head = yield* runCommand({
        ...command('git', ['-C', origin, 'rev-parse', 'HEAD']),
        buffered: true,
      });
      if (head.stdout.trim() !== revision) {
        return yield* Effect.fail(
          new TaskIcedSourceError({
            message: `Explicit iced source HEAD must be ${revision}. Only committed contents are fetched; the source checkout is never modified.`,
          }),
        );
      }
    }
    yield* Effect.acquireUseRelease(
      sourceIo(() => mkdtemp(path.join(VENDOR, '.iced-'))),
      (work) =>
        Effect.gen(function* () {
          const staged = path.join(work, 'checkout');
          yield* runCommand(command('git', ['init', staged]));
          yield* runCommand(
            command('git', ['-C', staged, 'fetch', '--depth=1', '--no-tags', origin, revision]),
          ).pipe(
            Effect.catchTag('TaskProcessError', (cause) =>
              Effect.fail(
                new TaskIcedSourceError({
                  message: `Cannot fetch pinned iced revision ${revision} from ${source === null ? remote : 'the explicit source'}. No substitute revision or source-tree edits are used.`,
                  cause,
                }),
              ),
            ),
          );
          yield* runCommand(command('git', ['-C', staged, 'checkout', '--detach', revision]));
          yield* verifyCheckout(staged, revision);
          yield* sourceIo(() => rename(staged, checkout));
        }),
      (work) => sourceIo(() => rm(work, { recursive: true, force: true })),
    );
  }
  yield* verifyCheckout(checkout, revision);
  yield* verifyStableLink(link);
  const current = yield* pathStatus(link);
  if (Option.isSome(current)) {
    const destination = path.resolve(VENDOR, yield* sourceIo(() => readlink(link)));
    if (destination === checkout) return;
  }
  // Rename only the link, preserving the previous checkout and any user edits.
  // A private temporary directory owns both publication and failure cleanup.
  yield* Effect.acquireUseRelease(
    sourceIo(() => mkdtemp(path.join(VENDOR, '.iced-link-'))),
    (work) =>
      sourceIo(async () => {
        const stagedLink = path.join(work, 'iced');
        await symlink(
          process.platform === 'win32' ? checkout : path.basename(checkout),
          stagedLink,
          process.platform === 'win32' ? 'junction' : 'dir',
        );
        await rename(stagedLink, link);
      }),
    (work) => sourceIo(() => rm(work, { recursive: true, force: true })),
  );
});
