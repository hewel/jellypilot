import { createHash, randomUUID } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { chmod, mkdir, mkdtemp, readFile, rm, stat, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { Data, Effect } from 'effect';

import { command } from './commands';
import { REPO_ROOT } from './constants';
import { buildIced } from './iced';
import { prepareIced } from './iced-source';
import { embeddedMpvEnvironment, MPV_ARTIFACT_ROOT } from './mpv';
import type { TaskCommand } from './parse';
import { runCommand } from './process';

type Scenario = 'tray' | 'external' | 'gpu';
type Status = 'pass' | 'fail' | 'unavailable';
type Request = Extract<TaskCommand, { readonly _tag: 'icedRegress' }>;
interface Result {
  readonly schemaVersion: 1;
  readonly runId: string;
  readonly scenario: Scenario;
  readonly status: Status;
  readonly checks: readonly string[];
  readonly error?: string;
  readonly native?: unknown;
}

class TaskNativeRegressionError extends Data.TaggedError('TaskNativeRegressionError')<{
  readonly message: string;
}> {}

const io = <A>(operation: () => Promise<A>) =>
  Effect.tryPromise({
    try: operation,
    catch: (cause) => new TaskNativeRegressionError({ message: String(cause) }),
  });
const jsonFile = async (file: string): Promise<unknown> => JSON.parse(await readFile(file, 'utf8'));
const save = (file: string, value: unknown) =>
  io(() => writeFile(file, `${JSON.stringify(value, null, 2)}\n`));
const hashFile = async (file: string): Promise<string> => {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(file)) hash.update(chunk);
  return hash.digest('hex');
};

function nativeReport(value: unknown, scenario: Scenario, runId: string): Result {
  if (
    typeof value !== 'object' ||
    value === null ||
    !('schemaVersion' in value) ||
    value.schemaVersion !== 1 ||
    !('runId' in value) ||
    value.runId !== runId ||
    !('scenario' in value) ||
    value.scenario !== scenario ||
    !('status' in value) ||
    (value.status !== 'pass' && value.status !== 'fail' && value.status !== 'unavailable') ||
    !('checks' in value) ||
    !Array.isArray(value.checks) ||
    !value.checks.every((check): check is string => typeof check === 'string')
  ) {
    throw new Error('Native regression report is missing, invalid, or belongs to a different run.');
  }
  return {
    schemaVersion: 1,
    runId,
    scenario,
    status: value.status,
    checks: value.checks,
    ...('error' in value && typeof value.error === 'string' ? { error: value.error } : {}),
    native: value,
  };
}

const child = (
  args: readonly string[],
  environment: Readonly<Record<string, string>>,
  timeout: `${number} seconds`,
  headless = false,
) =>
  runCommand({
    ...command(
      'dbus-run-session',
      [
        '--',
        ...(headless ? ['xvfb-run', '-a'] : []),
        path.join(REPO_ROOT, 'target/debug/jellypilot'),
        ...args,
      ],
      environment,
    ),
    acceptNonZero: true,
  }).pipe(Effect.timeout(timeout));

const runScenario = Effect.fn('task.nativeRegression.scenario')(function* (
  scenario: Scenario,
  runId: string,
  media: string | null,
  report: string,
  environment: Readonly<Record<string, string | undefined>>,
) {
  return yield* Effect.acquireUseRelease(
    io(() => mkdtemp(path.join(path.dirname(report), '.runtime-'))),
    (directory) =>
      Effect.gen(function* () {
        const runtime = path.join(directory, 'runtime');
        yield* io(() => mkdir(runtime, { mode: 0o700 }));
        yield* io(() => chmod(runtime, 0o700));
        const isolated = {
          XDG_CONFIG_HOME: path.join(directory, 'config'),
          XDG_DATA_HOME: path.join(directory, 'data'),
          XDG_CACHE_HOME: path.join(directory, 'cache'),
          XDG_RUNTIME_DIR: runtime,
          GIO_USE_VFS: 'local',
          NO_AT_BRIDGE: '1',
          WINIT_X11_SCALE_FACTOR: '1',
          JELLYPILOT_SMOKE_SIZE: '480x320',
        };
        if (scenario === 'external') {
          const absent = {
            ...isolated,
            WAYLAND_DISPLAY: '',
            WINIT_UNIX_BACKEND: 'x11',
            JELLYPILOT_LIBMPV: path.join(directory, 'absent-libmpv.so'),
            JELLYPILOT_MPV_BASELINE: path.join(directory, 'absent-baseline.conf'),
            VK_DRIVER_FILES: path.join(directory, 'absent-vulkan-driver.json'),
          };
          const rejected = yield* child(['--smoke-test', '--embedded'], absent, '60 seconds', true);
          if (rejected.exitCode === 0 || !rejected.stderr.includes('Embedded MPV asset missing:')) {
            return {
              schemaVersion: 1,
              runId,
              scenario,
              status: 'fail',
              checks: [],
              error:
                'Missing embedded assets did not produce the expected explicit startup rejection.',
            } satisfies Result;
          }
          const recovered = yield* child(
            ['--smoke-test', '--external'],
            absent,
            '60 seconds',
            true,
          );
          return {
            schemaVersion: 1,
            runId,
            scenario,
            status: recovered.exitCode === 0 ? 'pass' : 'fail',
            checks: [
              'missing embedded assets rejected without a system libmpv fallback',
              ...(recovered.exitCode === 0
                ? [
                    'explicit External startup succeeds with missing embedded library, baseline and Vulkan driver',
                    'daemon exits successfully',
                  ]
                : []),
            ],
            ...(recovered.exitCode === 0
              ? {}
              : { error: `External recovery exited with ${recovered.exitCode}.` }),
          } satisfies Result;
        }
        // Keep the actual display connection while isolating app/IPC state.
        // Wayland's relative socket name otherwise resolves under our private runtime directory.
        const wayland = environment.WAYLAND_DISPLAY;
        if (
          (!wayland && !environment.DISPLAY) ||
          (wayland && !path.isAbsolute(wayland) && !environment.XDG_RUNTIME_DIR)
        ) {
          return {
            schemaVersion: 1,
            runId,
            scenario,
            status: 'unavailable',
            checks: [],
            error:
              'Native tray/GPU probes require the current display and its Wayland runtime directory (or DISPLAY).',
          } satisfies Result;
        }
        const display: Readonly<Record<string, string>> = wayland
          ? { WAYLAND_DISPLAY: path.resolve(environment.XDG_RUNTIME_DIR ?? '/', wayland) }
          : {};
        const assets = yield* embeddedMpvEnvironment(environment);
        const locales = yield* runCommand({ ...command('locale', ['-a']), buffered: true });
        const numericLocale = locales.stdout
          .split(/\r?\n/)
          .find((locale) => locale !== '' && !/^(c|posix)([.@]|$)/i.test(locale));
        if (numericLocale === undefined) {
          return {
            schemaVersion: 1,
            runId,
            scenario,
            status: 'unavailable',
            checks: [],
            error: 'No installed non-C locale; install one before running the tray/GPU regression.',
          } satisfies Result;
        }
        const result = yield* child(
          [
            '--smoke-test',
            '--embedded',
            '--native-regression',
            scenario,
            '--regression-report',
            report,
            '--regression-run-id',
            runId,
            ...(media === null || scenario !== 'gpu' ? [] : ['--regression-media', media]),
          ],
          { ...isolated, ...assets, ...display, LC_ALL: numericLocale },
          '180 seconds',
        );
        const value = yield* io(() => jsonFile(report));
        const decoded = yield* Effect.try({
          try: () => nativeReport(value, scenario, runId),
          catch: (cause) => new TaskNativeRegressionError({ message: String(cause) }),
        });
        if (decoded.status === 'pass' && result.exitCode !== 0) {
          return {
            ...decoded,
            status: 'fail',
            error: `Native assertions reported pass but process exited with ${result.exitCode}.`,
          } satisfies Result;
        }
        return {
          ...decoded,
          checks: [
            ...decoded.checks,
            ...(decoded.status === 'pass'
              ? ['daemon exits successfully', `non-C environment locale: ${numericLocale}`]
              : []),
          ],
        };
      }),
    (directory) => Effect.promise(() => rm(directory, { recursive: true, force: true })),
  );
});

export const runNativeRegression = Effect.fn('task.nativeRegression')(function* (
  request: Request,
  environment: Readonly<Record<string, string | undefined>>,
) {
  // Reject missing input before preparing dependencies, building, or starting any process.
  const media = request.file === null ? null : path.resolve(REPO_ROOT, request.file);
  if (request.scenario === 'gpu' || request.scenario === 'all') {
    if (media === null)
      return yield* Effect.fail(
        new TaskNativeRegressionError({ message: 'gpu/all requires --file <media>.' }),
      );
    const info = yield* io(() => stat(media));
    if (!info.isFile())
      return yield* Effect.fail(
        new TaskNativeRegressionError({
          message: 'Regression media must be a readable regular local file.',
        }),
      );
  }
  const output = path.resolve(REPO_ROOT, request.output);
  yield* io(() => mkdir(output, { recursive: true }));
  const runId = randomUUID();
  const startedAt = new Date().toISOString();
  const scenarios: readonly Scenario[] =
    request.scenario === 'all' ? ['tray', 'external', 'gpu'] : [request.scenario];
  const results: Result[] = scenarios.map((scenario) => ({
    schemaVersion: 1,
    runId,
    scenario,
    status: 'unavailable',
    checks: [],
    error: 'Not executed in this run.',
  }));
  let sources: unknown = null;
  let mediaEvidence: unknown = null;
  let launcherSha256: string | null = null;
  const writeSummary = (finished: boolean) =>
    save(path.join(output, 'report.json'), {
      schemaVersion: 1,
      runId,
      startedAt,
      ...(finished ? { finishedAt: new Date().toISOString() } : {}),
      platform: process.platform,
      architecture: process.arch,
      sources,
      displayEnvironment: {
        display: environment.DISPLAY ?? null,
        waylandDisplay: environment.WAYLAND_DISPLAY ?? null,
        runtimeDirectory: environment.XDG_RUNTIME_DIR ?? null,
        vulkanDriverFiles: environment.VK_DRIVER_FILES ?? null,
        vulkanIcdFilenames: environment.VK_ICD_FILENAMES ?? null,
      },
      media: mediaEvidence,
      launcherSha256,
      status: results.some((result) => result.status === 'fail')
        ? 'fail'
        : results.some((result) => result.status === 'unavailable')
          ? 'unavailable'
          : 'pass',
      results,
      manualColorComparison: {
        status: 'unavailable',
        reason:
          'Automatic lifecycle probes do not perform human three-way SDR/HDR10/Profile5 color acceptance. References are never updated by this command.',
      },
    });
  for (const result of results) yield* save(path.join(output, `${result.scenario}.json`), result);
  yield* writeSummary(false);
  const prepared = yield* Effect.gen(function* () {
    if (process.platform !== 'linux')
      return yield* Effect.fail(
        new TaskNativeRegressionError({
          message:
            'Native regressions require Linux, a native display for tray/GPU, and a private D-Bus session.',
        }),
      );
    if (
      (environment.CARGO_TARGET_DIR !== undefined &&
        path.resolve(REPO_ROOT, environment.CARGO_TARGET_DIR) !== path.join(REPO_ROOT, 'target')) ||
      environment.CARGO_BUILD_TARGET
    ) {
      return yield* Effect.fail(
        new TaskNativeRegressionError({
          message:
            'Native regression requires the normal target/debug host build; unset CARGO_TARGET_DIR/CARGO_BUILD_TARGET overrides.',
        }),
      );
    }
    const application = yield* runCommand({
      ...command('git', ['rev-parse', 'HEAD']),
      buffered: true,
    });
    const changes = yield* runCommand({
      ...command('git', ['status', '--porcelain']),
      buffered: true,
    });
    const assets = yield* embeddedMpvEnvironment(environment).pipe(
      Effect.flatMap((paths) =>
        io(async () => ({
          library: {
            path: paths.JELLYPILOT_LIBMPV,
            sha256: await hashFile(paths.JELLYPILOT_LIBMPV),
          },
          baseline: {
            path: paths.JELLYPILOT_MPV_BASELINE,
            sha256: await hashFile(paths.JELLYPILOT_MPV_BASELINE),
          },
        })),
      ),
      Effect.match({
        onFailure: (error): unknown => ({ status: 'unavailable', error: String(error) }),
        onSuccess: (value): unknown => value,
      }),
    );
    sources = {
      application: {
        revision: application.stdout.trim(),
        workingTree: changes.stdout,
        dispatcherSha256: yield* io(() =>
          hashFile(path.join(REPO_ROOT, 'scripts/task/native-regression.ts')),
        ),
      },
      iced: yield* io(() => jsonFile(path.join(REPO_ROOT, 'tools/embedded-mpv/iced-source.json'))),
      mpv: yield* io(() => jsonFile(path.join(REPO_ROOT, 'tools/embedded-mpv/source.json'))),
      cargoLockSha256: yield* io(() => hashFile(path.join(REPO_ROOT, 'Cargo.lock'))),
      nativeBuild: yield* io(async () =>
        (await Bun.file(path.join(MPV_ARTIFACT_ROOT, 'manifest.json')).exists())
          ? jsonFile(path.join(MPV_ARTIFACT_ROOT, 'manifest.json'))
          : null,
      ),
      actualEmbeddedAssets: assets,
    };
    mediaEvidence =
      media === null
        ? null
        : {
            name: path.basename(media),
            sha256: yield* io(() => hashFile(media)),
            metadata: yield* runCommand({
              ...command('ffprobe', [
                '-v',
                'error',
                '-show_entries',
                'format=duration,format_name,size:stream=index,codec_type,codec_name,width,height,pix_fmt,color_range,color_space,color_transfer,color_primaries:stream_side_data',
                '-of',
                'json',
                media,
              ]),
              buffered: true,
            }).pipe(
              Effect.flatMap((result) =>
                Effect.try({
                  try: (): unknown => JSON.parse(result.stdout),
                  catch: () =>
                    new TaskNativeRegressionError({
                      message: 'ffprobe returned invalid media metadata.',
                    }),
                }),
              ),
            ),
          };
    yield* prepareIced(null);
    yield* buildIced(false);
    launcherSha256 = yield* io(() => hashFile(path.join(REPO_ROOT, 'target/debug/jellypilot')));
  }).pipe(Effect.result);
  if (prepared._tag === 'Failure') {
    for (let index = 0; index < results.length; index += 1) {
      const previous = results[index];
      if (previous !== undefined)
        results[index] = { ...previous, status: 'unavailable', error: String(prepared.failure) };
    }
  } else {
    for (let index = 0; index < scenarios.length; index += 1) {
      const scenario = scenarios[index];
      if (scenario === undefined) continue;
      const report = path.join(output, `${scenario}.json`);
      const result = yield* runScenario(scenario, runId, media, report, environment).pipe(
        Effect.catchTag('TaskMpvArtifactError', (error) =>
          Effect.succeed({
            schemaVersion: 1,
            runId,
            scenario,
            status: 'unavailable',
            checks: [],
            error: error.message,
          } satisfies Result),
        ),
        Effect.match({
          onFailure: (error): Result => ({
            schemaVersion: 1,
            runId,
            scenario,
            status: 'fail',
            checks: [],
            error: String(error),
          }),
          onSuccess: (value): Result => value,
        }),
      );
      results[index] = result;
      yield* save(report, result);
      yield* writeSummary(false);
    }
  }
  for (const result of results) yield* save(path.join(output, `${result.scenario}.json`), result);
  yield* writeSummary(true);
  yield* Effect.logInfo(`Native regression ${runId}: ${path.join(output, 'report.json')}`);
  if (results.some((result) => result.status !== 'pass')) {
    return yield* Effect.fail(
      new TaskNativeRegressionError({
        message:
          'Native regression did not pass every requested scenario; inspect the current run report.',
      }),
    );
  }
});
