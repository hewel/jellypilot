import { access, readFile, readdir, stat } from 'node:fs/promises';
import { homedir } from 'node:os';
import path from 'node:path';

import { Effect } from 'effect';

import { REPO_ROOT } from './constants';
import { TaskAndroidToolchainError } from './errors';

// Pinned Android toolchain inputs shared by the dispatcher commands and the
// native mpv pipeline (scripts/task/android-mpv.ts). Bump deliberately.
export const ANDROID_NDK_VERSION = '28.2.13676358';
export const ANDROID_BUILD_TOOLS_VERSION = '36.0.0';
export const ANDROID_MIN_API = 26;
export const ANDROID_COMPILE_SDK = 37;
export const ANDROID_RUST_TARGET = 'aarch64-linux-android';
export const ANDROID_ABI = 'arm64-v8a';

export const ANDROID_ROOT = path.join(REPO_ROOT, 'android');
export const ANDROID_MPV_ARTIFACT_ROOT = path.join(REPO_ROOT, 'target/android/mpv', ANDROID_ABI);
export const ANDROID_BINDINGS_DIR = path.join(ANDROID_ROOT, 'core-bridge/build/generated/uniffi');
export const ANDROID_JNILIBS_DIR = path.join(ANDROID_ROOT, 'core-bridge/build/jniLibs');

export interface AndroidToolchain {
  readonly sdkRoot: string;
  readonly ndkRoot: string;
  readonly llvmBinDir: string;
  readonly sysroot: string;
  readonly hostTag: string;
  readonly clangTriple: string;
  readonly ndkTriple: string;
  readonly apiLevel: number;
  readonly cmakeBinDir: string;
}

// Runs a promise to a nullable success value: any rejection becomes null.
// Used for filesystem probes where absence is data, not a failure.
export const attempt = <A>(operation: () => Promise<A>): Effect.Effect<A | null> =>
  Effect.tryPromise({ try: operation, catch: () => null }).pipe(
    Effect.catch(() => Effect.succeed(null)),
  );

const requireDirectory = Effect.fn('task.android.requireDirectory')(function* (
  prerequisite: string,
  location: string,
  hint: string,
) {
  const info = yield* attempt(() => stat(location));
  if (info === null || !info.isDirectory()) {
    return yield* new TaskAndroidToolchainError({
      prerequisite,
      message: `${prerequisite} not found at ${location}. ${hint}`,
    });
  }
  return location;
});

const executableOnPath = async (
  name: string,
  environment: Readonly<Record<string, string | undefined>>,
): Promise<string | null> => {
  const pathValue = environment.PATH ?? process.env.PATH ?? '';
  const executable = process.platform === 'win32' ? `${name}.exe` : name;
  for (const directory of pathValue.split(path.delimiter)) {
    if (directory.length === 0) continue;
    const candidate = path.join(directory, executable);
    try {
      await access(candidate);
      return candidate;
    } catch {
      continue;
    }
  }
  return null;
};

// Highest-versioned SDK cmake containing both cmake and ninja, else a PATH
// cmake whose directory also provides ninja. Anything else is a typed error.
const resolveCmakeBinDir = Effect.fn('task.android.cmake')(function* (
  sdkRoot: string,
  environment: Readonly<Record<string, string | undefined>>,
) {
  const cmakeRoot = path.join(sdkRoot, 'cmake');
  const entries = yield* attempt(() => readdir(cmakeRoot, { withFileTypes: true }));
  if (entries !== null) {
    const versions = entries
      .filter((entry) => entry.isDirectory())
      .map((entry) => entry.name)
      .toSorted((left, right) =>
        right.localeCompare(left, undefined, { numeric: true, sensitivity: 'base' }),
      );
    for (const version of versions) {
      const binDir = path.join(cmakeRoot, version, 'bin');
      const hasTools = yield* attempt(async () => {
        await access(path.join(binDir, 'cmake'));
        await access(path.join(binDir, 'ninja'));
        return true;
      });
      if (hasTools === true) return binDir;
    }
  }
  const pathCmake = yield* attempt(() => executableOnPath('cmake', environment));
  if (pathCmake !== null) {
    const binDir = path.dirname(pathCmake);
    const hasNinja = yield* attempt(async () => {
      await access(path.join(binDir, process.platform === 'win32' ? 'ninja.exe' : 'ninja'));
      return true;
    });
    if (hasNinja === true) return binDir;
  }
  return yield* new TaskAndroidToolchainError({
    prerequisite: 'cmake',
    message:
      'No usable cmake+ninja found. Install the SDK package with sdkmanager "cmake;3.22.1" or put cmake and ninja in the same directory on PATH.',
  });
});

// Resolves the pinned Android SDK/NDK toolchain. Environment accepts
// ANDROID_HOME/ANDROID_SDK_ROOT (falling back to ~/Android/Sdk) and
// ANDROID_NDK_HOME/ANDROID_NDK_ROOT (falling back to the pinned NDK inside the
// SDK). Missing prerequisites fail with actionable typed errors; nothing is
// downloaded or licensed implicitly.
export const resolveAndroidToolchain = Effect.fn('task.android.toolchain')(function* (
  environment: Readonly<Record<string, string | undefined>>,
) {
  const sdkRoot =
    environment.ANDROID_HOME ?? environment.ANDROID_SDK_ROOT ?? path.join(homedir(), 'Android/Sdk');
  yield* requireDirectory(
    'Android SDK',
    sdkRoot,
    'Set ANDROID_HOME or ANDROID_SDK_ROOT to an installed SDK, or install one at ~/Android/Sdk.',
  );
  const ndkRoot =
    environment.ANDROID_NDK_HOME ??
    environment.ANDROID_NDK_ROOT ??
    path.join(sdkRoot, 'ndk', ANDROID_NDK_VERSION);
  yield* requireDirectory(
    'Android NDK',
    ndkRoot,
    `Install the pinned NDK with sdkmanager "ndk;${ANDROID_NDK_VERSION}" or set ANDROID_NDK_HOME.`,
  );
  const properties = yield* attempt(() =>
    readFile(path.join(ndkRoot, 'source.properties'), 'utf8'),
  );
  const revision = properties?.match(/^Pkg\.Revision\s*=\s*(\S+)/m)?.[1];
  if (revision !== ANDROID_NDK_VERSION) {
    return yield* new TaskAndroidToolchainError({
      prerequisite: 'Android NDK revision',
      message: `Expected NDK ${ANDROID_NDK_VERSION} at ${ndkRoot}, found ${revision ?? 'no Pkg.Revision'}. Select the pinned installation with ANDROID_NDK_HOME.`,
    });
  }
  const hostTag =
    process.platform === 'linux'
      ? 'linux-x86_64'
      : process.platform === 'darwin'
        ? process.arch === 'arm64'
          ? 'darwin-arm64'
          : 'darwin-x86_64'
        : process.platform === 'win32'
          ? 'windows-x86_64'
          : null;
  if (hostTag === null) {
    return yield* new TaskAndroidToolchainError({
      prerequisite: 'host platform',
      message: `Unsupported Android NDK host platform ${process.platform}/${process.arch}.`,
    });
  }
  const llvmBinDir = yield* requireDirectory(
    'NDK LLVM toolchain',
    path.join(ndkRoot, 'toolchains/llvm/prebuilt', hostTag, 'bin'),
    `The NDK at ${ndkRoot} is incomplete; reinstall "ndk;${ANDROID_NDK_VERSION}".`,
  );
  const sysroot = yield* requireDirectory(
    'NDK sysroot',
    path.join(ndkRoot, 'toolchains/llvm/prebuilt', hostTag, 'sysroot'),
    `The NDK at ${ndkRoot} is incomplete; reinstall "ndk;${ANDROID_NDK_VERSION}".`,
  );
  const cmakeBinDir = yield* resolveCmakeBinDir(sdkRoot, environment);
  return {
    sdkRoot,
    ndkRoot,
    llvmBinDir,
    sysroot,
    hostTag,
    clangTriple: `${ANDROID_RUST_TARGET}${ANDROID_MIN_API}`,
    ndkTriple: ANDROID_RUST_TARGET,
    apiLevel: ANDROID_MIN_API,
    cmakeBinDir,
  } satisfies AndroidToolchain;
});

// Gradle-side prerequisites beyond the native toolchain: platform, build-tools
// and the checked-in wrapper. Used by android build/check and doctor.
export const resolveAndroidBuildEnvironment = Effect.fn('task.android.buildEnvironment')(function* (
  environment: Readonly<Record<string, string | undefined>>,
) {
  const toolchain = yield* resolveAndroidToolchain(environment);
  const platforms = yield* Effect.tryPromise({
    try: () => readdir(path.join(toolchain.sdkRoot, 'platforms')),
    catch: () => null,
  });
  const platform = platforms?.find((entry) => entry.startsWith(`android-${ANDROID_COMPILE_SDK}`));
  if (platform === undefined) {
    return yield* new TaskAndroidToolchainError({
      prerequisite: 'Android platform',
      message: `No platforms/android-${ANDROID_COMPILE_SDK}* under ${toolchain.sdkRoot}. Install it with sdkmanager "platforms;android-${ANDROID_COMPILE_SDK}.0".`,
    });
  }
  const buildTools = yield* requireDirectory(
    'Android build-tools',
    path.join(toolchain.sdkRoot, 'build-tools', ANDROID_BUILD_TOOLS_VERSION),
    `Install it with sdkmanager "build-tools;${ANDROID_BUILD_TOOLS_VERSION}".`,
  );
  const gradlew = path.join(ANDROID_ROOT, process.platform === 'win32' ? 'gradlew.bat' : 'gradlew');
  const gradlewInfo = yield* attempt(() => stat(gradlew));
  if (gradlewInfo === null || !gradlewInfo.isFile()) {
    return yield* new TaskAndroidToolchainError({
      prerequisite: 'Gradle wrapper',
      message: `${gradlew} is missing; the pinned Gradle wrapper must be checked in.`,
    });
  }
  return { toolchain, platform, buildTools, gradlew };
});
