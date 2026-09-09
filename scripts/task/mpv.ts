import { createHash } from 'node:crypto';
import { copyFile, mkdir, mkdtemp, readFile, stat, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { Effect } from 'effect';

import sourcePin from '../../tools/embedded-mpv/source.json';
import { command } from './commands';
import { REPO_ROOT } from './constants';
import { TaskMpvArtifactError, TaskMpvBuildError } from './errors';
import { runCommand } from './process';

export const MPV_ARTIFACT_ROOT = path.join(REPO_ROOT, 'target/embedded-mpv');
const LIBRARY = 'lib/jellypilot/libmpv.so';
const BASELINE = 'share/jellypilot/mpv-baseline.conf';

const buildIo = <A>(step: string, operation: () => Promise<A>) =>
  Effect.tryPromise({
    try: operation,
    catch: (cause) => new TaskMpvBuildError({ step, message: String(cause) }),
  });

const buildCommand = Effect.fn('task.mpv.command')(function* (
  step: string,
  executable: string,
  args: readonly string[],
  hint: string,
) {
  return yield* runCommand({ ...command(executable, args), buffered: true }).pipe(
    Effect.catchTag('TaskProcessError', (error) =>
      Effect.fail(
        new TaskMpvBuildError({
          step,
          message: `${hint}\n${error.message}`,
        }),
      ),
    ),
  );
});

const verifySource = Effect.fn('task.mpv.verifySource')(function* (source: string) {
  const head = yield* runCommand({
    ...command('git', ['-C', source, 'rev-parse', 'HEAD']),
    buffered: true,
  });
  if (head.stdout.trim() !== sourcePin.revision) {
    return yield* Effect.fail(
      new TaskMpvBuildError({
        step: 'source revision',
        message: `mpv checkout HEAD must be exactly ${sourcePin.revision}; found ${head.stdout.trim()}. The task never changes an explicit checkout.`,
      }),
    );
  }
  const status = yield* runCommand({
    ...command('git', ['-C', source, 'status', '--porcelain', '--untracked-files=no']),
    buffered: true,
  });
  if (status.stdout.trim() !== '') {
    return yield* Effect.fail(
      new TaskMpvBuildError({
        step: 'source cleanliness',
        message:
          'mpv checkout has tracked changes (including staged changes). Supply a clean pinned checkout; untracked build files are allowed.',
      }),
    );
  }
});

export const buildMpv = Effect.fn('task.mpv.build')(function* (source: string | null) {
  if (process.platform !== 'linux') {
    return yield* Effect.fail(
      new TaskMpvBuildError({
        step: 'platform',
        message: 'Embedded mpv currently requires Linux and Vulkan.',
      }),
    );
  }
  yield* buildIo('artifact directory', () => mkdir(MPV_ARTIFACT_ROOT, { recursive: true }));
  const work = yield* buildIo('build directory', () =>
    mkdtemp(path.join(MPV_ARTIFACT_ROOT, 'build-')),
  );
  const checkout = source === null ? path.join(work, 'source') : path.resolve(REPO_ROOT, source);
  if (source === null) {
    const hint = `Cannot retrieve pinned mpv ${sourcePin.revision} from ${sourcePin.remote}. If the commit is unpublished, use mpv build --source <clean-pinned-checkout>. No push is performed.`;
    yield* buildCommand(
      'clone',
      'git',
      ['clone', '--no-checkout', sourcePin.remote, checkout],
      hint,
    );
    yield* buildCommand(
      'fetch revision',
      'git',
      ['-C', checkout, 'fetch', '--depth=1', 'origin', sourcePin.revision],
      hint,
    );
    yield* buildCommand(
      'checkout revision',
      'git',
      ['-C', checkout, 'checkout', '--detach', sourcePin.revision],
      hint,
    );
  }
  yield* verifySource(checkout);

  const tools: Record<string, string> = {};
  for (const executable of ['git', 'meson', 'ninja', 'pkg-config', 'cc', 'c++', 'python3']) {
    const result = yield* buildCommand(
      'build prerequisite',
      executable,
      ['--version'],
      `Install ${executable} and ensure it is on PATH (Meson >=1.3 required).`,
    );
    tools[executable] = result.stdout.trim() || result.stderr.trim();
  }
  const packages: Record<string, string> = {};
  for (const requirement of sourcePin.requiredPackages) {
    const result = yield* buildCommand(
      'development dependency',
      'pkg-config',
      ['--modversion', requirement],
      `Install development packages for ${requirement}. Vulkan needs both loader and headers (vulkan-headers and vulkan-loader development packages); a runtime loader alone is insufficient. Use a documented SDK via PKG_CONFIG_PATH if not using system packages.`,
    );
    packages[requirement] = result.stdout.trim();
  }
  const build = path.join(work, 'meson');
  yield* runCommand(command('meson', ['setup', build, checkout, ...sourcePin.mesonOptions])).pipe(
    Effect.catchTag('TaskProcessError', (error) =>
      Effect.fail(
        new TaskMpvBuildError({
          step: 'Meson configure',
          message: `Meson configuration failed: ${error.message} See ${path.join(build, 'meson-logs/meson-log.txt')}. Install missing development headers/libraries, including Vulkan headers; no temporary header fallback or Meson dependency download is used.`,
        }),
      ),
    ),
  );
  yield* runCommand(command('meson', ['compile', '-C', build, 'mpv']));
  // Recheck after compilation to avoid certifying a checkout edited during the build.
  yield* verifySource(checkout);
  const baseline = yield* runCommand({
    ...command('git', ['-C', checkout, 'show', `${sourcePin.revision}:${sourcePin.baseline}`]),
    buffered: true,
  });
  const libraryPath = path.join(MPV_ARTIFACT_ROOT, LIBRARY);
  const baselinePath = path.join(MPV_ARTIFACT_ROOT, BASELINE);
  yield* buildIo('stage artifacts', async () => {
    await mkdir(path.dirname(libraryPath), { recursive: true });
    await mkdir(path.dirname(baselinePath), { recursive: true });
    await copyFile(path.join(build, 'libmpv.so'), libraryPath);
    await writeFile(baselinePath, baseline.stdout);
    const introspection: Record<string, unknown> = {};
    for (const name of ['dependencies', 'compilers', 'buildoptions', 'machines']) {
      const text = await readFile(path.join(build, `meson-info/intro-${name}.json`), 'utf8');
      const value: unknown = JSON.parse(text);
      introspection[name] = value;
    }
    const libraryHash = createHash('sha256')
      .update(await readFile(libraryPath))
      .digest('hex');
    const baselineHash = createHash('sha256')
      .update(await readFile(baselinePath))
      .digest('hex');
    await writeFile(
      path.join(MPV_ARTIFACT_ROOT, 'manifest.json'),
      `${JSON.stringify(
        {
          schemaVersion: 1,
          source: sourcePin,
          platform: process.platform,
          arch: process.arch,
          tools,
          packages,
          meson: introspection,
          artifacts: {
            [LIBRARY]: { sha256: libraryHash },
            [BASELINE]: { sha256: baselineHash },
          },
        },
        null,
        2,
      )}\n`,
    );
  });
  yield* Effect.logInfo(
    `Embedded mpv staged at ${MPV_ARTIFACT_ROOT}. ${sourcePin.reproducibility}`,
  );
});

export function embeddedMpvPaths(
  environment: Readonly<Record<string, string | undefined>>,
): Record<string, string> {
  return {
    JELLYPILOT_LIBMPV: environment.JELLYPILOT_LIBMPV ?? path.join(MPV_ARTIFACT_ROOT, LIBRARY),
    JELLYPILOT_MPV_BASELINE:
      environment.JELLYPILOT_MPV_BASELINE ?? path.join(MPV_ARTIFACT_ROOT, BASELINE),
  };
}

export const embeddedMpvEnvironment = Effect.fn('task.mpv.environment')(function* (
  environment: Readonly<Record<string, string | undefined>>,
) {
  const selected = embeddedMpvPaths(environment);
  for (const [name, value] of Object.entries(selected)) {
    if (!path.isAbsolute(value)) {
      return yield* Effect.fail(
        new TaskMpvArtifactError({
          path: value,
          message: `${name} must be an explicit absolute file path.`,
        }),
      );
    }
    const info = yield* Effect.tryPromise({
      try: () => stat(value),
      catch: () =>
        new TaskMpvArtifactError({
          path: value,
          message: `${name} file is unavailable. Run bun run task mpv build --source <clean-pinned-checkout> or supply an absolute override. No system libmpv fallback is permitted.`,
        }),
    });
    if (!info.isFile()) {
      return yield* Effect.fail(
        new TaskMpvArtifactError({ path: value, message: `${name} must point to a regular file.` }),
      );
    }
  }
  return selected;
});
