import { copyFile, mkdir, readdir, rm, stat } from 'node:fs/promises';
import { homedir } from 'node:os';
import path from 'node:path';

import { Effect, Match } from 'effect';

import { buildAndroidMpv } from './android-mpv';
import {
  ANDROID_ABI,
  ANDROID_BINDINGS_DIR,
  ANDROID_BUILD_TOOLS_VERSION,
  ANDROID_COMPILE_SDK,
  ANDROID_JNILIBS_DIR,
  ANDROID_MPV_ARTIFACT_ROOT,
  ANDROID_NDK_VERSION,
  ANDROID_ROOT,
  ANDROID_RUST_TARGET,
  attempt,
  resolveAndroidBuildEnvironment,
  resolveAndroidToolchain,
} from './android-toolchain';
import { command } from './commands';
import { REPO_ROOT } from './constants';
import { CARGO_MANIFESTS } from './crates';
import { TaskAndroidArtifactError, TaskAndroidToolchainError } from './errors';
import type { TaskCommand } from './parse';
import { runCommand } from './process';

type AndroidTask = Extract<TaskCommand, { readonly _tag: `android${string}` }>;

export function isAndroidTask(task: TaskCommand): task is AndroidTask {
  return task._tag.startsWith('android');
}

const FFI_PACKAGE = 'jellypilot-ffi';
const FFI_LIBRARY = 'libjellypilot_ffi.so';
const BINDGEN_BINARY = 'jellypilot-bindgen';

const artifactIo = <A>(location: string, operation: () => Promise<A>) =>
  Effect.tryPromise({
    try: operation,
    catch: (cause) =>
      new TaskAndroidArtifactError({
        path: location,
        message: cause instanceof Error ? cause.message : String(cause),
      }),
  });

const requireFile = Effect.fn('task.android.requireFile')(function* (
  location: string,
  hint: string,
) {
  const info = yield* attempt(() => stat(location));
  if (info === null || !info.isFile()) {
    return yield* new TaskAndroidArtifactError({
      path: location,
      message: `${location} is missing. ${hint}`,
    });
  }
});

const hostFfiLibrary = (): string => {
  const name =
    process.platform === 'darwin'
      ? 'libjellypilot_ffi.dylib'
      : process.platform === 'win32'
        ? 'jellypilot_ffi.dll'
        : FFI_LIBRARY;
  return path.join(REPO_ROOT, 'target/debug', name);
};

// Regenerates the Kotlin UniFFI bindings into core-bridge's build tree. The
// ffi crate's jellypilot-bindgen binary forwards conventional uniffi-bindgen
// generate arguments; the namespace comes from the crate's uniffi.toml.
export const runAndroidBindings = Effect.fn('task.android.bindings')(function* () {
  yield* runCommand(
    command('cargo', [
      'build',
      '--locked',
      '--manifest-path',
      CARGO_MANIFESTS.shared,
      '--package',
      FFI_PACKAGE,
    ]),
  );
  const library = hostFfiLibrary();
  yield* requireFile(
    library,
    `cargo build -p ${FFI_PACKAGE} produced no cdylib; check the crate-type in crates/jellypilot-ffi/Cargo.toml.`,
  );
  yield* artifactIo(ANDROID_BINDINGS_DIR, () =>
    rm(ANDROID_BINDINGS_DIR, { recursive: true, force: true }),
  );
  yield* runCommand(
    command('cargo', [
      'run',
      '--locked',
      '--manifest-path',
      CARGO_MANIFESTS.shared,
      '--package',
      FFI_PACKAGE,
      '--bin',
      BINDGEN_BINARY,
      '--',
      'generate',
      '--library',
      library,
      '--language',
      'kotlin',
      '--out-dir',
      ANDROID_BINDINGS_DIR,
      '--no-format',
    ]),
  );
  const generated = yield* artifactIo(ANDROID_BINDINGS_DIR, async () => {
    const files: string[] = [];
    for await (const file of new Bun.Glob('**/*.kt').scan({
      cwd: ANDROID_BINDINGS_DIR,
      onlyFiles: true,
    })) {
      files.push(file);
    }
    return files;
  });
  if (generated.length === 0) {
    return yield* new TaskAndroidArtifactError({
      path: ANDROID_BINDINGS_DIR,
      message: `${BINDGEN_BINARY} produced no Kotlin sources under ${ANDROID_BINDINGS_DIR}.`,
    });
  }
  yield* Effect.logInfo(`Generated ${generated.length} Kotlin binding file(s).`);
});

// Cross-compiles the shared ffi cdylib for arm64 Android and stages it where
// the core-bridge module picks it up as a jniLibs source directory.
export const runAndroidRust = Effect.fn('task.android.rust')(function* (
  release: boolean,
  environment: Readonly<Record<string, string | undefined>>,
) {
  const toolchain = yield* resolveAndroidToolchain(environment);
  const targetEnv = ANDROID_RUST_TARGET.replaceAll('-', '_').toUpperCase();
  const ccEnv = ANDROID_RUST_TARGET.replaceAll('-', '_');
  yield* runCommand(
    command(
      'cargo',
      [
        'build',
        '--locked',
        '--manifest-path',
        CARGO_MANIFESTS.shared,
        '--package',
        FFI_PACKAGE,
        '--target',
        ANDROID_RUST_TARGET,
        ...(release ? ['--release'] : []),
      ],
      {
        [`CARGO_TARGET_${targetEnv}_LINKER`]: path.join(
          toolchain.llvmBinDir,
          `${toolchain.clangTriple}-clang`,
        ),
        [`CC_${ccEnv}`]: path.join(toolchain.llvmBinDir, `${toolchain.clangTriple}-clang`),
        [`AR_${ccEnv}`]: path.join(toolchain.llvmBinDir, 'llvm-ar'),
        [`RANLIB_${ccEnv}`]: path.join(toolchain.llvmBinDir, 'llvm-ranlib'),
      },
    ),
  );
  const built = path.join(
    REPO_ROOT,
    'target',
    ANDROID_RUST_TARGET,
    release ? 'release' : 'debug',
    FFI_LIBRARY,
  );
  yield* requireFile(
    built,
    `cargo build -p ${FFI_PACKAGE} --target ${ANDROID_RUST_TARGET} produced no ${FFI_LIBRARY}; check the crate-type in crates/jellypilot-ffi/Cargo.toml.`,
  );
  const destination = path.join(ANDROID_JNILIBS_DIR, ANDROID_ABI, FFI_LIBRARY);
  yield* artifactIo(destination, async () => {
    await mkdir(path.dirname(destination), { recursive: true });
    await copyFile(built, destination);
  });
  yield* Effect.logInfo(`Staged ${FFI_LIBRARY} at ${destination}.`);
});

export const runAndroidMpv = Effect.fn('task.android.mpv')(function* (
  environment: Readonly<Record<string, string | undefined>>,
) {
  yield* buildAndroidMpv(environment);
  yield* requireFile(
    path.join(ANDROID_MPV_ARTIFACT_ROOT, 'lib/libmpv.so'),
    'The native pipeline finished without producing libmpv.so; inspect the manifest beside target/android/mpv/arm64-v8a.',
  );
});

const gradleEnvironment = (sdkRoot: string, ndkRoot: string): Record<string, string> => ({
  ANDROID_HOME: sdkRoot,
  ANDROID_SDK_ROOT: sdkRoot,
  ANDROID_NDK_HOME: ndkRoot,
});

// Full APK path: bindings, Rust cdylib and the native mpv pipeline all run
// before Gradle assemble — a full build never silently skips native libraries.
export const runAndroidBuild = Effect.fn('task.android.build')(function* (
  release: boolean,
  environment: Readonly<Record<string, string | undefined>>,
) {
  const { toolchain, gradlew } = yield* resolveAndroidBuildEnvironment(environment);
  yield* runAndroidBindings();
  yield* runAndroidRust(release, environment);
  yield* runAndroidMpv(environment);
  yield* runCommand(
    command(
      gradlew,
      ['-p', 'android', release ? 'assembleRelease' : 'assembleDebug'],
      gradleEnvironment(toolchain.sdkRoot, toolchain.ndkRoot),
    ),
  );
});

// Gradle verification (lint + unit tests) with fresh generated bindings; Rust
// crate checks stay under `rust check sdk ffi`.
export const runAndroidCheck = Effect.fn('task.android.check')(function* (
  environment: Readonly<Record<string, string | undefined>>,
) {
  const { toolchain, gradlew } = yield* resolveAndroidBuildEnvironment(environment);
  yield* runAndroidBindings();
  yield* runCommand(
    command(
      gradlew,
      ['-p', 'android', 'check'],
      gradleEnvironment(toolchain.sdkRoot, toolchain.ndkRoot),
    ),
  );
});

interface DoctorCheck {
  readonly label: string;
  readonly required: boolean;
  readonly ok: boolean;
  readonly detail: string;
}

const probe = (
  label: string,
  required: boolean,
  detail: Effect.Effect<string, unknown>,
): Effect.Effect<DoctorCheck> =>
  detail.pipe(
    Effect.match({
      onFailure: (error): DoctorCheck => ({
        label,
        required,
        ok: false,
        detail: error instanceof Error ? error.message : String(error),
      }),
      onSuccess: (text): DoctorCheck => ({ label, required, ok: true, detail: text }),
    }),
  );
const pathDetail = (directory: boolean, location: string, hint: string) =>
  attempt(() => stat(location)).pipe(
    Effect.flatMap((info) =>
      info !== null && (directory ? info.isDirectory() : info.isFile())
        ? Effect.succeed(location)
        : Effect.fail(
            new TaskAndroidToolchainError({
              prerequisite: location,
              message: `missing at ${location}. ${hint}`,
            }),
          ),
    ),
  );

// Reports every Android prerequisite with an actionable detail; fails when any
// required piece is missing. Informational rows (staged artifacts, sdkmanager)
// never fail the run.
export const runAndroidDoctor = Effect.fn('task.android.doctor')(function* (
  environment: Readonly<Record<string, string | undefined>>,
) {
  const sdkRoot =
    environment.ANDROID_HOME ?? environment.ANDROID_SDK_ROOT ?? path.join(homedir(), 'Android/Sdk');
  const probes = [
    probe(
      'JDK',
      true,
      runCommand({
        ...command('java', ['-version']),
        buffered: true,
        acceptNonZero: true,
      }).pipe(
        Effect.flatMap((result) =>
          result.exitCode === 0
            ? Effect.succeed(result.stderr.split('\n')[0]?.trim() ?? 'java available')
            : Effect.fail(
                new TaskAndroidToolchainError({
                  prerequisite: 'JDK',
                  message: `java -version exited ${result.exitCode}; install a JDK on PATH or set JAVA_HOME.`,
                }),
              ),
        ),
      ),
    ),
    probe(
      'Android SDK',
      true,
      pathDetail(
        true,
        sdkRoot,
        'Set ANDROID_HOME or ANDROID_SDK_ROOT to an installed SDK, or install one at ~/Android/Sdk.',
      ),
    ),
    probe(
      `platform android-${ANDROID_COMPILE_SDK}`,
      true,
      attempt(() => readdir(path.join(sdkRoot, 'platforms'))).pipe(
        Effect.flatMap((entries) => {
          const found = entries?.find((entry) =>
            entry.startsWith(`android-${ANDROID_COMPILE_SDK}`),
          );
          return found === undefined
            ? Effect.fail(
                new TaskAndroidToolchainError({
                  prerequisite: 'Android platform',
                  message: `install with sdkmanager "platforms;android-${ANDROID_COMPILE_SDK}.0".`,
                }),
              )
            : Effect.succeed(`platforms/${found}`);
        }),
      ),
    ),
    probe(
      `build-tools ${ANDROID_BUILD_TOOLS_VERSION}`,
      true,
      pathDetail(
        true,
        path.join(sdkRoot, 'build-tools', ANDROID_BUILD_TOOLS_VERSION),
        `install with sdkmanager "build-tools;${ANDROID_BUILD_TOOLS_VERSION}".`,
      ),
    ),
    probe(
      `NDK ${ANDROID_NDK_VERSION} + cmake`,
      true,
      resolveAndroidToolchain(environment).pipe(
        Effect.map(
          (toolchain) =>
            `${toolchain.ndkRoot} (host ${toolchain.hostTag}, cmake ${toolchain.cmakeBinDir})`,
        ),
      ),
    ),
    probe(
      `rust target ${ANDROID_RUST_TARGET}`,
      true,
      runCommand({
        ...command('rustup', ['target', 'list', '--installed']),
        buffered: true,
        acceptNonZero: true,
      }).pipe(
        Effect.flatMap((result) =>
          result.exitCode === 0 && result.stdout.split('\n').includes(ANDROID_RUST_TARGET)
            ? Effect.succeed(ANDROID_RUST_TARGET)
            : Effect.fail(
                new TaskAndroidToolchainError({
                  prerequisite: 'rustup target',
                  message: `run rustup target add ${ANDROID_RUST_TARGET}.`,
                }),
              ),
        ),
      ),
    ),
    probe(
      FFI_PACKAGE,
      true,
      pathDetail(
        false,
        path.join(REPO_ROOT, 'crates/jellypilot-ffi/Cargo.toml'),
        'the shared ffi crate is required for bindings and the Rust cross-build.',
      ),
    ),
    probe(
      'Gradle wrapper',
      true,
      pathDetail(
        false,
        path.join(ANDROID_ROOT, process.platform === 'win32' ? 'gradlew.bat' : 'gradlew'),
        'the pinned Gradle wrapper must be checked in under android/.',
      ),
    ),
    probe(
      'sdkmanager',
      false,
      attempt(async () => {
        for (const candidate of [
          path.join(sdkRoot, 'cmdline-tools/latest/bin/sdkmanager'),
          path.join(sdkRoot, 'cmdline-tools/23.0/bin/sdkmanager'),
        ]) {
          const info = await stat(candidate).catch(() => null);
          if (info !== null && info.isFile()) return candidate;
        }
        return null;
      }).pipe(
        Effect.flatMap((found) =>
          found !== null
            ? Effect.succeed(found)
            : Effect.fail(
                new TaskAndroidToolchainError({
                  prerequisite: 'sdkmanager',
                  message: 'optional; install cmdline-tools to manage SDK packages.',
                }),
              ),
        ),
      ),
    ),
    probe(
      `staged ${FFI_LIBRARY}`,
      false,
      pathDetail(
        false,
        path.join(ANDROID_JNILIBS_DIR, ANDROID_ABI, FFI_LIBRARY),
        'run bun run task android rust.',
      ),
    ),
    probe(
      'staged libmpv.so',
      false,
      pathDetail(
        false,
        path.join(ANDROID_MPV_ARTIFACT_ROOT, 'lib/libmpv.so'),
        'run bun run task android mpv.',
      ),
    ),
    probe(
      'generated Kotlin bindings',
      false,
      attempt(async () => {
        const files: string[] = [];
        for await (const file of new Bun.Glob('**/*.kt').scan({
          cwd: ANDROID_BINDINGS_DIR,
          onlyFiles: true,
        })) {
          files.push(file);
        }
        return files;
      }).pipe(
        Effect.flatMap((files) =>
          files !== null && files.length > 0
            ? Effect.succeed(`${files.length} file(s) under ${ANDROID_BINDINGS_DIR}`)
            : Effect.fail(
                new TaskAndroidToolchainError({
                  prerequisite: 'bindings',
                  message: 'run bun run task android bindings.',
                }),
              ),
        ),
      ),
    ),
  ];
  const checks = yield* Effect.all(probes, { concurrency: 'unbounded' });

  yield* Effect.sync(() => {
    console.log('\n=== android doctor ===');
    for (const check of checks) {
      const mark = check.ok ? 'ok' : check.required ? 'MISSING' : 'absent';
      console.log(`[${mark}] ${check.label}: ${check.detail}`);
    }
  });

  const missing = checks.filter((check) => check.required && !check.ok);
  if (missing.length > 0) {
    return yield* new TaskAndroidToolchainError({
      prerequisite: 'android doctor',
      message: `Missing required Android prerequisites:\n${missing
        .map((check) => `- ${check.label}: ${check.detail}`)
        .join('\n')}`,
    });
  }
});

// Single dispatcher for every `android` subcommand; keeps the task.ts Match
// under its branch limit and groups the Android surface in one place.
export const runAndroid = Effect.fn('task.android')(
  (task: AndroidTask, environment: Readonly<Record<string, string | undefined>>) =>
    Match.value(task).pipe(
      Match.when({ _tag: 'androidDoctor' }, () => runAndroidDoctor(environment)),
      Match.when({ _tag: 'androidBindings' }, () => runAndroidBindings()),
      Match.when({ _tag: 'androidRust' }, ({ release }) => runAndroidRust(release, environment)),
      Match.when({ _tag: 'androidMpv' }, () => runAndroidMpv(environment)),
      Match.when({ _tag: 'androidBuild' }, ({ release }) => runAndroidBuild(release, environment)),
      Match.when({ _tag: 'androidCheck' }, () => runAndroidCheck(environment)),
      Match.exhaustive,
    ),
);
