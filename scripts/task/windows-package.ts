import { createHash } from 'node:crypto';
import { copyFile, cp, mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { Data, Effect, Result, Schema } from 'effect';

import sourcePin from '../../tools/embedded-mpv/source.json';
import { command } from './commands';
import { REPO_ROOT } from './constants';
import { buildIced } from './iced';
import { prepareIced } from './iced-source';
import { MPV_ARTIFACT_ROOT } from './mpv';
import { runCommand } from './process';
import { readWindowsPeDump, stageWindowsDependencies } from './windows-dependencies';

class TaskWindowsPackageError extends Data.TaggedError('TaskWindowsPackageError')<{
  readonly message: string;
}> {}

const packageIo = <A>(operation: () => Promise<A>) =>
  Effect.tryPromise({
    try: operation,
    catch: (cause) => new TaskWindowsPackageError({ message: String(cause) }),
  });

const Manifest = Schema.Struct({
  schemaVersion: Schema.Literal(1),
  platform: Schema.Literal('win32'),
  arch: Schema.Literal('x64'),
  source: Schema.Struct({ revision: Schema.Literal(sourcePin.revision) }),
  artifacts: Schema.Record(Schema.String, Schema.Struct({ sha256: Schema.String })),
});

const LIBRARY = 'lib/jellypilot/libmpv-2.dll';
const BASELINE = 'share/jellypilot/mpv-baseline.conf';
const NOTICES = ['Copyright', 'LICENSE.GPL', 'LICENSE.LGPL'].map(
  (name) => `share/jellypilot/licenses/mpv/${name}`,
);

export const packageWindows = Effect.fn('task.package.windows')(function* (
  runtimeDirectory: string,
  environment: Readonly<Record<string, string | undefined>>,
) {
  const systemRoot = environment.SystemRoot ?? environment.SYSTEMROOT;
  if (process.platform !== 'win32' || process.arch !== 'x64' || systemRoot === undefined) {
    return yield* Effect.fail(
      new TaskWindowsPackageError({ message: 'Windows packaging requires native x64 Windows.' }),
    );
  }
  const toolRoot = path.join(REPO_ROOT, 'target/tools/cargo-packager');
  const localPackager = path.join(toolRoot, 'bin/cargo-packager.exe');
  let packagerExecutable = localPackager;
  let packagerReady = false;
  for (const candidate of ['cargo-packager', localPackager]) {
    const version = yield* runCommand({
      ...command(candidate, ['--version']),
      buffered: true,
    }).pipe(Effect.result);
    if (Result.isSuccess(version) && /\b0\.11\.8\b/.test(version.success.stdout)) {
      packagerExecutable = candidate;
      packagerReady = true;
      break;
    }
  }
  if (!packagerReady) {
    yield* runCommand(
      command('cargo', [
        'install',
        'cargo-packager',
        '--version',
        '0.11.8',
        '--locked',
        '--root',
        toolRoot,
      ]),
    );
  }
  const manifestText = yield* packageIo(() =>
    readFile(path.join(MPV_ARTIFACT_ROOT, 'manifest.json'), 'utf8'),
  );
  const manifest = yield* Schema.decodeUnknownEffect(Manifest)(
    yield* packageIo(async (): Promise<unknown> => JSON.parse(manifestText)),
  ).pipe(
    Effect.catchTag('SchemaError', (cause) =>
      Effect.fail(new TaskWindowsPackageError({ message: `Rebuild Windows mpv: ${cause}` })),
    ),
  );
  for (const relative of [LIBRARY, BASELINE, ...NOTICES]) {
    const contents = yield* packageIo(() => readFile(path.join(MPV_ARTIFACT_ROOT, relative)));
    const actual = createHash('sha256').update(contents).digest('hex');
    if (manifest.artifacts[relative]?.sha256 !== actual) {
      return yield* Effect.fail(
        new TaskWindowsPackageError({
          message: `Staged ${relative} does not match the mpv manifest. Run bun run task mpv build.`,
        }),
      );
    }
    if (relative === BASELINE) {
      const current = yield* packageIo(() =>
        readFile(path.join(REPO_ROOT, 'tools/embedded-mpv/baseline-windows.conf')),
      );
      if (!contents.equals(current)) {
        return yield* Effect.fail(
          new TaskWindowsPackageError({
            message: 'Windows baseline changed; rebuild mpv before packaging.',
          }),
        );
      }
    }
  }
  // A fresh payload excludes DLLs left behind by an older dependency graph.
  const work = yield* packageIo(async () => {
    const parent = path.join(REPO_ROOT, 'target/windows-package');
    await mkdir(parent, { recursive: true });
    return mkdtemp(path.join(parent, 'build-'));
  });
  const runtime = path.resolve(REPO_ROOT, runtimeDirectory);
  yield* packageIo(() => mkdir(path.join(work, 'lib/jellypilot'), { recursive: true }));
  const dependencyManifest = yield* stageWindowsDependencies(
    path.join(MPV_ARTIFACT_ROOT, LIBRARY),
    runtime,
    path.join(work, 'lib/jellypilot'),
    path.join(systemRoot, 'System32'),
  );
  const vswhere = path.join(
    environment['ProgramFiles(x86)'] ?? 'C:/Program Files (x86)',
    'Microsoft Visual Studio/Installer/vswhere.exe',
  );
  const redist = yield* runCommand({
    ...command(vswhere, [
      '-latest',
      '-products',
      '*',
      '-requires',
      'Microsoft.VisualStudio.Component.VC.Tools.x86.x64',
      '-find',
      'VC/Redist/MSVC/**/x64/Microsoft.VC*.CRT/vcruntime140.dll',
    ]),
    buffered: true,
  });
  const vcRuntime = redist.stdout
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line !== '')
    .toSorted((left, right) => left.localeCompare(right, 'en', { numeric: true }))
    .at(-1);
  if (vcRuntime === undefined) {
    return yield* Effect.fail(
      new TaskWindowsPackageError({
        message: 'Install the Visual Studio x64 C++ redistributable build files.',
      }),
    );
  }
  yield* prepareIced(null);
  yield* buildIced(true);
  const launcherDirectory = path.join(work, 'launcher');
  yield* packageIo(() => mkdir(launcherDirectory));
  const launcherManifest = yield* stageWindowsDependencies(
    path.join(REPO_ROOT, 'target/release/jellypilot.exe'),
    path.dirname(vcRuntime),
    launcherDirectory,
    path.join(systemRoot, 'System32'),
    (file) => readWindowsPeDump(runtime, file),
  );
  const configText = yield* packageIo(async () => {
    const share = path.join(work, 'share/jellypilot');
    await mkdir(share, { recursive: true });
    for (const relative of [BASELINE, ...NOTICES]) {
      const destination = path.join(work, relative);
      await mkdir(path.dirname(destination), { recursive: true });
      await copyFile(path.join(MPV_ARTIFACT_ROOT, relative), destination);
    }
    // MSYS2 distributes package notices separately from DLLs. Preserve that tree.
    await cp(path.join(runtime, '../share/licenses'), path.join(share, 'licenses/msys2'), {
      recursive: true,
      errorOnExist: true,
    });
    await writeFile(path.join(share, 'mpv-manifest.json'), manifestText);
    await writeFile(
      path.join(share, 'windows-runtime.json'),
      `${JSON.stringify({ schemaVersion: 1, ...dependencyManifest, launcher: launcherManifest }, null, 2)}\n`,
    );
    const base = Bun.TOML.parse(await readFile(path.join(REPO_ROOT, 'Packager.toml'), 'utf8'));
    const config = {
      ...base,
      // Build before collecting launcher imports; do not rebuild after recording its hashes.
      'before-packaging-command': null,
      resources: [
        { src: path.join(work, 'lib/jellypilot'), target: 'lib/jellypilot' },
        { src: share, target: 'share/jellypilot' },
        ...launcherManifest.bundled
          .filter(({ name }) => name !== 'jellypilot.exe')
          .map(({ name }) => ({ src: path.join(launcherDirectory, name), target: name })),
      ],
      'out-dir': path.join(REPO_ROOT, 'target/release/bundle'),
    };
    const text = JSON.stringify(config);
    await writeFile(path.join(work, 'Packager.json'), `${JSON.stringify(config, null, 2)}\n`);
    return text;
  });
  // 0.11.8 changes cwd for a config file. Inline JSON keeps root-relative hooks/assets valid.
  yield* runCommand(command(packagerExecutable, ['--config', configText, '--formats', 'nsis']));
  yield* Effect.logInfo(`Windows NSIS package built with embedded resources from ${work}.`);
});
