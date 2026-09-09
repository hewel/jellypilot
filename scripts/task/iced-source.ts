import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, realpath, rename, symlink, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { Data, Effect } from 'effect';

import { command } from './commands';
import { REPO_ROOT } from './constants';
import { runCommand } from './process';

const BASE_REVISION = '3e32da72f7faf6e9b16f0b259756aba3c3ed0b96';
const REMOTE = 'https://github.com/hewel/iced.git';
const VENDOR = path.join(REPO_ROOT, 'target/vendor');

class TaskIcedSourceError extends Data.TaggedError('TaskIcedSourceError')<{
  readonly message: string;
}> {}

const sourceIo = <A>(operation: () => Promise<A>) =>
  Effect.tryPromise({
    try: operation,
    catch: (cause) =>
      new TaskIcedSourceError({ message: `iced source preparation failed: ${String(cause)}` }),
  });

export const prepareIced = Effect.fn('task.iced.prepare')(function* (source: string | null) {
  const pinPath = path.join(REPO_ROOT, 'tools/embedded-mpv/iced-source.json');
  const patchPath = path.join(REPO_ROOT, 'tools/embedded-mpv/iced.patch');
  const pin: unknown = yield* sourceIo(async () => JSON.parse(await readFile(pinPath, 'utf8')));
  if (
    typeof pin !== 'object' ||
    pin === null ||
    !('revision' in pin) ||
    pin.revision !== BASE_REVISION ||
    !('remote' in pin) ||
    pin.remote !== REMOTE ||
    !('patchSha256' in pin) ||
    typeof pin.patchSha256 !== 'string' ||
    !/^[a-f0-9]{64}$/.test(pin.patchSha256)
  ) {
    return yield* Effect.fail(
      new TaskIcedSourceError({
        message: `Invalid ${pinPath}: expected the accepted iced base revision, remote, and actual patchSha256. No substitute revision is allowed.`,
      }),
    );
  }
  const patch = yield* sourceIo(() => readFile(patchPath));
  if (createHash('sha256').update(patch).digest('hex') !== pin.patchSha256) {
    return yield* Effect.fail(
      new TaskIcedSourceError({
        message:
          'iced.patch SHA256 does not match iced-source.json. Capture and review the exact extension patch before preparation.',
      }),
    );
  }
  yield* sourceIo(() => mkdir(VENDOR, { recursive: true }));
  const checkout = path.join(VENDOR, `iced-${pin.patchSha256}`);
  const marker = path.join(checkout, '.jellypilot-tree');
  const exists = yield* sourceIo(() => Bun.file(marker).exists());
  if (!exists) {
    const work = yield* sourceIo(() => mkdtemp(path.join(VENDOR, '.iced-')));
    const origin = source === null ? REMOTE : path.resolve(REPO_ROOT, source);
    if (source !== null) {
      const head = yield* runCommand({
        ...command('git', ['-C', origin, 'rev-parse', 'HEAD']),
        buffered: true,
      });
      if (head.stdout.trim() !== BASE_REVISION) {
        return yield* Effect.fail(
          new TaskIcedSourceError({
            message: `Explicit iced source HEAD must be ${BASE_REVISION}. Only committed base contents are copied; the checked-in patch supplies the extension.`,
          }),
        );
      }
    }
    yield* runCommand(
      command('git', ['clone', '--no-checkout', '--no-hardlinks', origin, work]),
    ).pipe(
      Effect.catchTag('TaskProcessError', () =>
        Effect.fail(
          new TaskIcedSourceError({
            message: `Cannot clone iced. If the accepted base is unpublished, run bun run task iced prepare --source <checkout-at-${BASE_REVISION}>. No push or source-checkout modification is performed.`,
          }),
        ),
      ),
    );
    yield* runCommand(command('git', ['-C', work, 'checkout', '--detach', BASE_REVISION])).pipe(
      Effect.catchTag('TaskProcessError', () =>
        Effect.fail(
          new TaskIcedSourceError({
            message: `Accepted iced base ${BASE_REVISION} is unavailable. Use iced prepare --source <checkout> containing that exact commit.`,
          }),
        ),
      ),
    );
    yield* runCommand(command('git', ['-C', work, 'apply', '--index', patchPath]));
    const tree = yield* runCommand({
      ...command('git', ['-C', work, 'write-tree']),
      buffered: true,
    });
    yield* sourceIo(() => writeFile(path.join(work, '.jellypilot-tree'), tree.stdout.trim()));
    yield* sourceIo(() => rename(work, checkout));
  }
  const tree = (yield* sourceIo(() => readFile(marker, 'utf8'))).trim();
  if (!/^[a-f0-9]{40}$/.test(tree)) {
    return yield* Effect.fail(
      new TaskIcedSourceError({
        message:
          'Prepared iced tree marker is invalid. Remove only the generated target/vendor iced checkout and prepare it again.',
      }),
    );
  }
  yield* runCommand({
    ...command('git', ['-C', checkout, 'diff', '--exit-code', tree]),
    buffered: true,
  }).pipe(
    Effect.catchTag('TaskProcessError', () =>
      Effect.fail(
        new TaskIcedSourceError({
          message:
            'Prepared iced source has been edited. Do not build an uncertified extension; remove only the generated target/vendor iced checkout and prepare again.',
        }),
      ),
    ),
  );
  // Direct path dependencies avoid Cargo resolving an unpublished git revision
  // even when a [patch] table replaces every upstream package.
  yield* sourceIo(async () => {
    const link = path.join(VENDOR, 'iced');
    if (await Bun.file(path.join(link, 'Cargo.toml')).exists()) {
      if ((await realpath(link)) !== (await realpath(checkout))) {
        throw new Error(
          'Prepared iced link targets a different patch; remove only target/vendor/iced and prepare again.',
        );
      }
      return;
    }
    await symlink(
      process.platform === 'win32' ? checkout : path.basename(checkout),
      link,
      process.platform === 'win32' ? 'junction' : 'dir',
    );
  });
});
