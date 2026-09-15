import { createHash } from 'node:crypto';
import { constants as fsConstants } from 'node:fs';
import {
  cp,
  mkdir,
  readdir,
  readFile,
  realpath,
  rename,
  rm,
  stat,
  symlink,
  writeFile,
} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';

import { Data, Effect, Schema } from 'effect';

import depsPinJson from '../../tools/android/mpv-deps.json';
import { resolveAndroidToolchain, type AndroidToolchain } from './android-toolchain';
import { command } from './commands';
import { REPO_ROOT } from './constants';
import { runCommand } from './process';

export class TaskAndroidMpvError extends Data.TaggedError('TaskAndroidMpvError')<{
  readonly step: string;
  readonly message: string;
}> {}

export interface AndroidMpvArtifacts {
  readonly artifactRoot: string;
  readonly includeDir: string;
  readonly libDir: string;
  readonly libraries: readonly string[];
  readonly manifestPath: string;
}

const TarballSourcePin = Schema.Struct({
  type: Schema.Literal('tarball'),
  url: Schema.String,
  sha256: Schema.String,
});

const GitSourcePin = Schema.Struct({
  type: Schema.Literal('git'),
  remote: Schema.String,
  commit: Schema.String,
  tag: Schema.optional(Schema.String),
  note: Schema.optional(Schema.String),
  submodules: Schema.optional(Schema.Boolean),
});

const SourcePatch = Schema.Struct({ file: Schema.String, sha256: Schema.String });

const DepPin = Schema.Struct({
  name: Schema.String,
  version: Schema.String,
  source: Schema.Union([TarballSourcePin, GitSourcePin]),
  licenseFiles: Schema.Array(Schema.String),
  mesonOptions: Schema.optional(Schema.Array(Schema.String)),
  configureOptions: Schema.optional(Schema.Array(Schema.String)),
  patch: Schema.optional(SourcePatch),
});

const ManifestPin = Schema.Struct({
  schemaVersion: Schema.Literal(1),
  abi: Schema.String,
  apiLevel: Schema.Number,
  ndkVersion: Schema.String,
  cpuFamily: Schema.String,
  cpu: Schema.String,
  deps: Schema.Array(DepPin),
  mpv: Schema.Struct({
    source: GitSourcePin,
    licenseFiles: Schema.Array(Schema.String),
    mesonOptions: Schema.Array(Schema.String),
    patch: Schema.optional(SourcePatch),
  }),
  reproducibility: Schema.String,
});

// The pin file is committed source data; a malformed edit is a defect and
// fails loudly at import rather than mid-build.
const pin = Schema.decodeUnknownSync(ManifestPin)(depsPinJson);

export const ANDROID_MPV_ARTIFACT_ROOT = path.join(REPO_ROOT, 'target/android/mpv', pin.abi);
const BUILD_ROOT = path.join(REPO_ROOT, 'target/android/mpv/build');
const SOURCE_ROOT = path.join(REPO_ROOT, 'target/android/mpv/sources');
const CACHE_ROOT = path.join(REPO_ROOT, 'target/android/mpv/cache');
const PREFIX_DIR = path.join(BUILD_ROOT, 'prefix');
const CROSSFILE = path.join(PREFIX_DIR, 'crossfile.txt');

// Host environment variables that would leak host headers/libraries or flags
// into the cross build. Unset for every child process, mirroring mpv-android's
// loadarch() which clears the same set.
const POLLUTED_ENV = [
  'CPATH',
  'LIBRARY_PATH',
  'C_INCLUDE_PATH',
  'CPLUS_INCLUDE_PATH',
  'CFLAGS',
  'CXXFLAGS',
  'CPPFLAGS',
  'PKG_CONFIG_PATH',
];

const HOST_TOOLS = [
  'git',
  'tar',
  'make',
  'meson',
  'ninja',
  'pkg-config',
  'python3',
  'gperf',
  'env',
  'patch',
];

// Android system libraries a packaged .so may legitimately depend on.
// libc++_shared.so is deliberately absent: it is an NDK runtime library that
// must be packaged, and the NEEDED scan copies it from the sysroot on demand.
const SYSTEM_LIBS: Record<string, true> = {
  'libaaudio.so': true,
  'libandroid.so': true,
  'libc.so': true,
  'libcamera2ndk.so': true,
  'libdl.so': true,
  'libEGL.so': true,
  'libGLESv1_CM.so': true,
  'libGLESv2.so': true,
  'libGLESv3.so': true,
  'libjnigraphics.so': true,
  'liblog.so': true,
  'libm.so': true,
  'libmediandk.so': true,
  'libOpenSLES.so': true,
  'libvulkan.so': true,
  'libz.so': true,
};
const MIN_LOAD_ALIGNMENT = 0x40_00; // 16 KB page-size compatibility
const SOURCE_DIFF_ARGS = [
  'diff',
  '--no-ext-diff',
  '--no-textconv',
  '--binary',
  '--ignore-submodules=untracked',
  'HEAD',
];

const io = <A>(step: string, operation: () => Promise<A>) =>
  Effect.tryPromise({
    try: operation,
    catch: (cause) =>
      new TaskAndroidMpvError({
        step,
        message: cause instanceof Error ? cause.message : String(cause),
      }),
  });

interface RunOptions {
  readonly step: string;
  readonly cwd?: string;
  readonly env?: Readonly<Record<string, string>>;
  readonly unsetCc?: boolean;
  readonly buffered?: boolean;
  readonly acceptNonZero?: boolean;
}

const run = Effect.fn('task.androidMpv.run')(function* (
  executable: string,
  args: readonly string[],
  options: RunOptions,
) {
  const unsets = [...POLLUTED_ENV, ...(options.unsetCc === true ? ['CC', 'CXX'] : [])];
  const argv = [
    ...unsets.flatMap((name) => ['-u', name]),
    ...(options.cwd === undefined ? [] : ['-C', options.cwd]),
    executable,
    ...args,
  ];
  return yield* runCommand({
    ...command('env', argv, options.env),
    buffered: options.buffered ?? false,
    acceptNonZero: options.acceptNonZero ?? false,
  }).pipe(
    Effect.catchTag('TaskProcessError', (error) =>
      Effect.fail(
        new TaskAndroidMpvError({
          step: options.step,
          message: `${executable} failed: ${error.message}`,
        }),
      ),
    ),
  );
});

const exists = (target: string) =>
  stat(target)
    .then(() => true)
    .catch(() => false);

const sha256File = (step: string, file: string) =>
  io(step, async () =>
    createHash('sha256')
      .update(await readFile(file))
      .digest('hex'),
  );

const buildEnv = (toolchain: AndroidToolchain): Record<string, string> => ({
  PATH: `${toolchain.llvmBinDir}:${process.env.PATH ?? ''}`,
  CC: `${toolchain.clangTriple}-clang`,
  CXX: `${toolchain.clangTriple}-clang++`,
  AR: 'llvm-ar',
  RANLIB: 'llvm-ranlib',
  LDFLAGS: '-Wl,-O1,--icf=safe -Wl,-z,max-page-size=16384',
  PKG_CONFIG_SYSROOT_DIR: PREFIX_DIR,
  PKG_CONFIG_LIBDIR: `${PREFIX_DIR}/lib/pkgconfig`,
});

const probeHostTools = Effect.fn('task.androidMpv.probeHostTools')(function* (
  toolchain: AndroidToolchain,
) {
  const tools: Record<string, string> = {};
  for (const executable of HOST_TOOLS) {
    const result = yield* run(executable, ['--version'], {
      step: 'host prerequisite',
      buffered: true,
    });
    tools[executable] = result.stdout.trim().split('\n')[0];
  }
  const clang = yield* run(`${toolchain.clangTriple}-clang`, ['--version'], {
    step: 'NDK clang',
    env: { PATH: `${toolchain.llvmBinDir}:${process.env.PATH ?? ''}` },
    buffered: true,
  });
  tools[`${toolchain.clangTriple}-clang`] = clang.stdout.trim().split('\n')[0];
  const cmake = yield* run(path.join(toolchain.cmakeBinDir, 'cmake'), ['--version'], {
    step: 'CMake prerequisite',
    buffered: true,
  });
  tools.cmake = cmake.stdout.trim().split('\n')[0];
  return tools;
});

const fetchTarball = Effect.fn('task.androidMpv.fetchTarball')(function* (
  dep: string,
  source: typeof TarballSourcePin.Type,
) {
  const file = path.join(CACHE_ROOT, path.basename(new URL(source.url).pathname));
  if (yield* io('source cache', () => exists(file))) {
    const cached = yield* sha256File('source cache', file);
    if (cached === source.sha256) return file;
    yield* Effect.logWarning(
      `Cached ${dep} archive has sha256 ${cached}, expected ${source.sha256}; downloading again.`,
    );
    yield* io('source cache', () => rm(file));
  }
  const partial = `${file}.partial`;
  yield* io(`download ${dep}`, async () => {
    const response = await fetch(source.url);
    if (!response.ok) {
      throw new Error(`HTTP ${response.status} for ${source.url}`);
    }
    await writeFile(partial, Buffer.from(await response.arrayBuffer()));
  });
  const downloaded = yield* sha256File(`verify ${dep}`, partial);
  if (downloaded !== source.sha256) {
    yield* io('source cache', () => rm(partial, { force: true }));
    return yield* Effect.fail(
      new TaskAndroidMpvError({
        step: `verify ${dep}`,
        message: `Downloaded ${source.url} has sha256 ${downloaded}, expected ${source.sha256}. Refusing to build from an unverified source.`,
      }),
    );
  }
  yield* io('source cache', () => rename(partial, file));
  return file;
});

const fetchGitSource = Effect.fn('task.androidMpv.fetchGitSource')(function* (
  dep: string,
  source: typeof GitSourcePin.Type,
) {
  const dir = path.join(SOURCE_ROOT, dep);
  if (!(yield* io('source checkout', () => exists(path.join(dir, '.git'))))) {
    yield* io('source checkout', async () => {
      await rm(dir, { recursive: true, force: true });
      await mkdir(dir, { recursive: true });
    });
    yield* run('git', ['-C', dir, 'init'], { step: `clone ${dep}` });
  }
  const hint = `Cannot retrieve pinned ${dep} commit ${source.commit} from ${source.remote}.`;
  const present = yield* run('git', ['-C', dir, 'cat-file', '-e', `${source.commit}^{commit}`], {
    step: `fetch ${dep}`,
    buffered: true,
    acceptNonZero: true,
  });
  if (present.exitCode !== 0) {
    const shallow = yield* run(
      'git',
      ['-C', dir, 'fetch', '--depth=1', source.remote, source.commit],
      { step: `fetch ${dep}`, buffered: true, acceptNonZero: true },
    );
    if (shallow.exitCode !== 0 && source.tag !== undefined) {
      yield* run('git', ['-C', dir, 'fetch', '--depth=1', source.remote, 'tag', source.tag], {
        step: `fetch ${dep}`,
        buffered: true,
        acceptNonZero: true,
      });
    }
    const fetched = yield* run('git', ['-C', dir, 'cat-file', '-e', `${source.commit}^{commit}`], {
      step: `fetch ${dep}`,
      buffered: true,
      acceptNonZero: true,
    });
    if (fetched.exitCode !== 0) {
      yield* run('git', ['-C', dir, 'fetch', source.remote, source.commit], {
        step: `fetch ${dep}`,
      }).pipe(
        Effect.catchTag('TaskAndroidMpvError', (error) =>
          Effect.fail(
            new TaskAndroidMpvError({
              step: `fetch ${dep}`,
              message: `${hint} ${error.message}`,
            }),
          ),
        ),
      );
    }
  }
  yield* run('git', ['-C', dir, 'checkout', '--detach', source.commit], {
    step: `checkout ${dep}`,
  });
  yield* run('git', ['-C', dir, 'clean', '-fdx'], { step: `checkout ${dep}` });
  if (source.submodules === true) {
    // Submodule commits are pinned by the superproject gitlinks, so this
    // stays immutable; --recommend-shallow keeps the fetch small.
    yield* run('git', ['-C', dir, 'submodule', 'update', '--init', '--recommend-shallow'], {
      step: `submodules ${dep}`,
    });
  }
  return dir;
});

const verifyGitSource = Effect.fn('task.androidMpv.verifyGitSource')(function* (
  dep: string,
  dir: string,
  commit: string,
  expectedDiff = '',
) {
  const head = yield* run('git', ['-C', dir, 'rev-parse', 'HEAD'], {
    step: `${dep} source revision`,
    buffered: true,
  });
  if (head.stdout.trim() !== commit) {
    return yield* Effect.fail(
      new TaskAndroidMpvError({
        step: `${dep} source revision`,
        message: `${dep} checkout HEAD must be exactly ${commit}; found ${head.stdout.trim()}.`,
      }),
    );
  }
  const diff = yield* run('git', ['-C', dir, ...SOURCE_DIFF_ARGS], {
    step: `${dep} source integrity`,
    buffered: true,
  });
  if (diff.stdout !== expectedDiff) {
    return yield* Effect.fail(
      new TaskAndroidMpvError({
        step: `${dep} source integrity`,
        message: `${dep} checkout has unexpected tracked changes.`,
      }),
    );
  }
});

const preparePatchedSource = Effect.fn('task.androidMpv.patchSource')(function* (
  dep: Pick<typeof DepPin.Type, 'name' | 'patch'>,
  sourceDir: string,
) {
  if (dep.patch === undefined) return sourceDir;
  const patchFile = path.join(REPO_ROOT, dep.patch.file);
  const digest = yield* sha256File('source patch', patchFile);
  if (digest !== dep.patch.sha256) {
    return yield* Effect.fail(
      new TaskAndroidMpvError({
        step: 'source patch',
        message: `Patch digest mismatch: ${dep.patch.file}.`,
      }),
    );
  }
  // Keep the fetched checkout pristine; copied Git metadata lets the final
  // integrity check distinguish the declared patch from later source edits.
  const directory = path.join(BUILD_ROOT, `patched-${dep.name}`);
  yield* io('patched source', async () => {
    await rm(directory, { recursive: true, force: true });
    await cp(sourceDir, directory, {
      recursive: true,
      mode: fsConstants.COPYFILE_FICLONE,
      filter: (candidate) => path.basename(candidate) !== '_build',
    });
  });
  yield* run('patch', ['--batch', '--forward', '-p1', '--input', patchFile], {
    step: `patch ${dep.name}`,
    cwd: directory,
  });
  return directory;
});

const prepareSource = Effect.fn('task.androidMpv.prepareSource')(function* (
  dep: typeof DepPin.Type,
) {
  const dir = path.join(SOURCE_ROOT, dep.name);
  if (dep.source.type === 'git') {
    const checkout = yield* fetchGitSource(dep.name, dep.source);
    yield* verifyGitSource(dep.name, checkout, dep.source.commit);
    return checkout;
  }
  const archive = yield* fetchTarball(dep.name, dep.source);
  yield* io(`extract ${dep.name}`, async () => {
    await rm(dir, { recursive: true, force: true });
    await mkdir(dir, { recursive: true });
  });
  yield* run('tar', ['-xf', archive, '-C', dir, '--strip-components=1'], {
    step: `extract ${dep.name}`,
  });
  return dir;
});

const mesonBuild = Effect.fn('task.androidMpv.mesonBuild')(function* (
  dep: string,
  sourceDir: string,
  options: readonly string[],
  toolchain: AndroidToolchain,
) {
  const buildDir = path.join(sourceDir, '_build');
  yield* io(`configure ${dep}`, () => rm(buildDir, { recursive: true, force: true }));
  yield* run('meson', ['setup', buildDir, sourceDir, '--cross-file', CROSSFILE, ...options], {
    step: `configure ${dep}`,
    env: buildEnv(toolchain),
    unsetCc: true,
  });
  yield* run('ninja', ['-C', buildDir, `-j${os.cpus().length}`], {
    step: `compile ${dep}`,
    env: buildEnv(toolchain),
    unsetCc: true,
  });
  yield* run('ninja', ['-C', buildDir, 'install'], {
    step: `install ${dep}`,
    env: { ...buildEnv(toolchain), DESTDIR: PREFIX_DIR },
    unsetCc: true,
  });
});

const configureBuild = Effect.fn('task.androidMpv.configureBuild')(function* (
  dep: typeof DepPin.Type,
  sourceDir: string,
  toolchain: AndroidToolchain,
  extraArgs: readonly string[] = [],
) {
  const buildDir = path.join(sourceDir, '_build');
  yield* io(`configure ${dep.name}`, async () => {
    await rm(buildDir, { recursive: true, force: true });
    await mkdir(buildDir, { recursive: true });
  });
  yield* run(path.join(sourceDir, 'configure'), [...(dep.configureOptions ?? []), ...extraArgs], {
    step: `configure ${dep.name}`,
    cwd: buildDir,
    env: buildEnv(toolchain),
  });
  yield* run('make', [`-j${os.cpus().length}`], {
    step: `compile ${dep.name}`,
    cwd: buildDir,
    env: buildEnv(toolchain),
  });
  yield* run('make', ['install', `DESTDIR=${PREFIX_DIR}`], {
    step: `install ${dep.name}`,
    cwd: buildDir,
    env: buildEnv(toolchain),
  });
});

const buildMbedtls = Effect.fn('task.androidMpv.mbedtls')(function* (
  sourceDir: string,
  toolchain: AndroidToolchain,
) {
  // mbedtls only supports in-source builds; the tarball is re-extracted every
  // run so the tree is already clean.
  yield* run('python3', ['scripts/config.py', 'set', 'MBEDTLS_AESNI_C'], {
    step: 'configure mbedtls',
    cwd: sourceDir,
    env: buildEnv(toolchain),
  });
  yield* run(
    'python3',
    ['scripts/config.py', 'set', 'MBEDTLS_PLATFORM_DEV_RANDOM', '"/dev/urandom"'],
    { step: 'configure mbedtls', cwd: sourceDir, env: buildEnv(toolchain) },
  );
  yield* run('make', [`-j${os.cpus().length}`, 'no_test'], {
    step: 'compile mbedtls',
    cwd: sourceDir,
    env: buildEnv(toolchain),
  });
  yield* run('make', ['install', `DESTDIR=${PREFIX_DIR}`], {
    step: 'install mbedtls',
    cwd: sourceDir,
    env: buildEnv(toolchain),
  });
});

const buildFfmpeg = Effect.fn('task.androidMpv.ffmpeg')(function* (
  dep: typeof DepPin.Type,
  sourceDir: string,
  toolchain: AndroidToolchain,
) {
  yield* configureBuild(dep, sourceDir, toolchain, [
    `--cross-prefix=${toolchain.ndkTriple}-`,
    `--cc=${toolchain.clangTriple}-clang`,
    `--arch=${pin.cpuFamily}`,
    `--cpu=${pin.cpu}`,
    `--extra-cflags=-I${PREFIX_DIR}/include`,
    `--extra-ldflags=-L${PREFIX_DIR}/lib -Wl,-z,max-page-size=16384`,
  ]);
});

const buildGlslang = Effect.fn('task.androidMpv.glslang')(function* (
  sourceDir: string,
  toolchain: AndroidToolchain,
) {
  const buildDir = path.join(sourceDir, '_build');
  const cmake = path.join(toolchain.cmakeBinDir, 'cmake');
  yield* io('configure glslang', () => rm(buildDir, { recursive: true, force: true }));
  yield* run(
    cmake,
    [
      '-S',
      sourceDir,
      '-B',
      buildDir,
      '-G',
      'Ninja',
      `-DCMAKE_MAKE_PROGRAM=${path.join(toolchain.cmakeBinDir, 'ninja')}`,
      `-DCMAKE_TOOLCHAIN_FILE=${path.join(toolchain.ndkRoot, 'build/cmake/android.toolchain.cmake')}`,
      `-DANDROID_ABI=${pin.abi}`,
      `-DANDROID_PLATFORM=android-${pin.apiLevel}`,
      '-DANDROID_STL=c++_shared',
      '-DCMAKE_BUILD_TYPE=Release',
      `-DCMAKE_INSTALL_PREFIX=${PREFIX_DIR}`,
      '-DCMAKE_INSTALL_LIBDIR=lib',
      '-DCMAKE_POSITION_INDEPENDENT_CODE=ON',
      '-DBUILD_SHARED_LIBS=OFF',
      '-DBUILD_EXTERNAL=OFF',
      '-DENABLE_OPT=OFF',
      '-DENABLE_HLSL=OFF',
      '-DENABLE_GLSLANG_BINARIES=OFF',
      '-DGLSLANG_TESTS=OFF',
      '-DGLSLANG_ENABLE_INSTALL=ON',
    ],
    { step: 'configure glslang', env: buildEnv(toolchain), unsetCc: true },
  );
  yield* run(cmake, ['--build', buildDir, '--parallel', String(os.cpus().length)], {
    step: 'compile glslang',
    env: buildEnv(toolchain),
    unsetCc: true,
  });
  yield* run(cmake, ['--install', buildDir], {
    step: 'install glslang',
    env: buildEnv(toolchain),
    unsetCc: true,
  });
});

// The loader is Android's system libvulkan; headers come from libplacebo's
// recursively pinned Vulkan-Headers submodule, never from the host SDK.
const stageVulkanHeaders = (sourceDir: string) =>
  io('Vulkan headers', async () => {
    const include = path.join(sourceDir, '3rdparty/Vulkan-Headers/include');
    const header = await readFile(path.join(include, 'vulkan/vulkan_core.h'), 'utf8');
    const patch = header.match(/^#define VK_HEADER_VERSION (\d+)$/m);
    const api = header.match(
      /^#define VK_HEADER_VERSION_COMPLETE VK_MAKE_API_VERSION\(\d+, (\d+), (\d+), VK_HEADER_VERSION\)$/m,
    );
    if (patch === null || api === null) throw new Error('Pinned Vulkan header version is missing.');
    await cp(include, path.join(PREFIX_DIR, 'include'), { recursive: true });
    await mkdir(path.join(PREFIX_DIR, 'lib/pkgconfig'), { recursive: true });
    await writeFile(
      path.join(PREFIX_DIR, 'lib/pkgconfig/vulkan.pc'),
      [
        'Name: Vulkan',
        'Description: Android system Vulkan loader with pinned Vulkan-Headers',
        `Version: ${api[1]}.${api[2]}.${patch[1]}`,
        `Cflags: -I${PREFIX_DIR}/include`,
        'Libs: -lvulkan',
        '',
      ].join('\n'),
    );
  });

// libplacebo is C++; when linked statically into libmpv.so the C++ runtime is
// missing because of a meson pkg-config generation bug
// (https://github.com/mesonbuild/meson/issues/11300). mpv-android appends
// -lc++ to the installed .pc for the same reason.
const fixLibplaceboPkgconfig = Effect.fn('task.androidMpv.libplaceboPc')(function* () {
  const pc = path.join(PREFIX_DIR, 'lib/pkgconfig/libplacebo.pc');
  const content = yield* io('libplacebo pkg-config', () => readFile(pc, 'utf8'));
  if (content.includes('-lc++')) return;
  const patched = content.replace(/^Libs:(.*)$/m, 'Libs:$1 -lc++');
  if (patched === content) {
    return yield* Effect.fail(
      new TaskAndroidMpvError({
        step: 'libplacebo pkg-config',
        message: `${pc} has no Libs: line to append -lc++ to.`,
      }),
    );
  }
  yield* io('libplacebo pkg-config', () => writeFile(pc, patched));
});

const writeCrossfile = (toolchain: AndroidToolchain) =>
  io('cross file', () =>
    writeFile(
      CROSSFILE,
      [
        '[built-in options]',
        "buildtype = 'release'",
        "default_library = 'static'",
        "wrap_mode = 'nodownload'",
        "prefix = '/usr/local'",
        `c_args = ['-I${PREFIX_DIR}/include']`,
        `cpp_args = ['-I${PREFIX_DIR}/include']`,
        `c_link_args = ['-L${PREFIX_DIR}/lib', '-Wl,-z,max-page-size=16384']`,
        `cpp_link_args = ['-L${PREFIX_DIR}/lib', '-Wl,-z,max-page-size=16384']`,
        '[binaries]',
        `c = '${toolchain.clangTriple}-clang'`,
        `cpp = '${toolchain.clangTriple}-clang++'`,
        "ar = 'llvm-ar'",
        "nm = 'llvm-nm'",
        "strip = 'llvm-strip'",
        "pkgconfig = 'pkg-config'",
        "pkg-config = 'pkg-config'",
        '[host_machine]',
        "system = 'android'",
        `cpu_family = '${pin.cpuFamily}'`,
        `cpu = '${toolchain.ndkTriple.split('-')[0]}'`,
        "endian = 'little'",
        '[properties]',
        'needs_exe_wrapper = true',
        '',
      ].join('\n'),
    ),
  );

const readelf = (toolchain: AndroidToolchain, args: readonly string[]) =>
  run(path.join(toolchain.llvmBinDir, 'llvm-readelf'), args, {
    step: 'inspect artifacts',
    buffered: true,
  });

const elfField = (output: string, tag: string) => {
  const match = output.match(new RegExp(`\\(${tag}\\).*\\[(.+?)\\]`));
  return match === null ? null : match[1];
};

const stageArtifacts = Effect.fn('task.androidMpv.stage')(function* (
  toolchain: AndroidToolchain,
  sourceDirs: Readonly<Record<string, string>>,
) {
  const libDir = path.join(ANDROID_MPV_ARTIFACT_ROOT, 'lib');
  const includeDir = path.join(ANDROID_MPV_ARTIFACT_ROOT, 'include');
  const noticesDir = path.join(ANDROID_MPV_ARTIFACT_ROOT, 'notices');
  yield* io('stage artifacts', async () => {
    await rm(ANDROID_MPV_ARTIFACT_ROOT, { recursive: true, force: true });
    await mkdir(libDir, { recursive: true });
    await mkdir(noticesDir, { recursive: true });
  });

  // Stage every installed shared object under both its SONAME and its plain
  // lib*.so name: the dynamic loader needs the SONAME while JNI consumers link
  // and System.loadLibrary() against the unversioned name.
  const prefixLib = path.join(PREFIX_DIR, 'lib');
  const installed = yield* io('stage libraries', () => readdir(prefixLib));
  const staged = new Set<string>();
  for (const entry of installed.filter((name) => /\.so($|\.)/.test(name))) {
    const real = yield* io('stage libraries', () => realpath(path.join(prefixLib, entry)));
    const dynamic = yield* readelf(toolchain, ['-dW', real]);
    const soname = elfField(dynamic.stdout, 'SONAME');
    const names = new Set<string>();
    if (soname !== null) names.add(soname);
    names.add(path.basename(real).replace(/\.so(\..*)?$/, '.so'));
    for (const name of names) {
      yield* io('stage libraries', () => cp(real, path.join(libDir, name)));
      staged.add(name);
    }
  }

  yield* io('stage headers', () =>
    cp(path.join(PREFIX_DIR, 'include'), includeDir, { recursive: true }),
  );

  // Package the NDK C++ runtime when any staged library needs it; it is not an
  // Android system library.
  const scanNeeded = Effect.fn('task.androidMpv.scanNeeded')(function* () {
    const needed = new Set<string>();
    for (const name of staged) {
      const dynamic = yield* readelf(toolchain, ['-dW', path.join(libDir, name)]);
      for (const entry of dynamic.stdout.matchAll(/\(NEEDED\).*\[(.+?)\]/g)) {
        needed.add(entry[1]);
      }
    }
    return needed;
  });
  let needed = yield* scanNeeded();
  if (needed.has('libc++_shared.so') && !staged.has('libc++_shared.so')) {
    const runtime = path.join(
      toolchain.sysroot,
      'usr/lib',
      toolchain.ndkTriple,
      'libc++_shared.so',
    );
    yield* io('stage libc++', () => cp(runtime, path.join(libDir, 'libc++_shared.so')));
    staged.add('libc++_shared.so');
    needed = yield* scanNeeded();
  }
  const missing = [...needed].filter((name) => !staged.has(name) && !(name in SYSTEM_LIBS));
  if (missing.length > 0) {
    return yield* Effect.fail(
      new TaskAndroidMpvError({
        step: 'runtime dependencies',
        message: `Staged libraries require unpackaged dependencies: ${missing.join(', ')}. Extend the pipeline or the system-library allowlist deliberately.`,
      }),
    );
  }

  // 16 KB page-size compatibility: every LOAD segment must be aligned to at
  // least 16 KB (enforced via -Wl,-z,max-page-size=16384 at link time).
  for (const name of staged) {
    const programHeaders = yield* readelf(toolchain, ['-lW', path.join(libDir, name)]);
    for (const line of programHeaders.stdout.split('\n')) {
      const fields = line.trim().split(/\s+/);
      if (fields[0] !== 'LOAD') continue;
      const align = Number.parseInt(fields[fields.length - 1], 16);
      if (!(align >= MIN_LOAD_ALIGNMENT)) {
        return yield* Effect.fail(
          new TaskAndroidMpvError({
            step: '16 KB alignment',
            message: `${name} has a LOAD segment aligned to ${fields[fields.length - 1]} (< 0x4000).`,
          }),
        );
      }
    }
  }

  const licensed: readonly (readonly [string, readonly string[]])[] = [
    ...pin.deps.map((dep) => [dep.name, dep.licenseFiles] as readonly [string, readonly string[]]),
    ['mpv', pin.mpv.licenseFiles],
  ];
  for (const [dep, files] of licensed) {
    for (const licenseFile of files) {
      const source = path.join(sourceDirs[dep], licenseFile);
      if (!(yield* io('license material', () => exists(source)))) {
        return yield* Effect.fail(
          new TaskAndroidMpvError({
            step: 'license material',
            message: `Pinned license file ${licenseFile} is missing from the ${dep} source.`,
          }),
        );
      }
      const destination = path.join(noticesDir, dep, path.basename(licenseFile));
      yield* io('license material', async () => {
        await mkdir(path.dirname(destination), { recursive: true });
        await cp(source, destination);
      });
    }
  }
  for (const dep of [...pin.deps, { name: 'mpv', patch: pin.mpv.patch }]) {
    const patch = dep.patch;
    if (patch === undefined) continue;
    const destination = path.join(
      ANDROID_MPV_ARTIFACT_ROOT,
      'source-patches',
      dep.name,
      path.basename(patch.file),
    );
    yield* io('stage source patch', async () => {
      await mkdir(path.dirname(destination), { recursive: true });
      await cp(path.join(REPO_ROOT, patch.file), destination);
    });
  }
  for (const notice of ['NOTICE', 'NOTICE.toolchain']) {
    const source = path.join(toolchain.ndkRoot, notice);
    if (yield* io('license material', () => exists(source))) {
      yield* io('license material', () => cp(source, path.join(noticesDir, 'android-ndk', notice)));
    }
  }
  return { libDir, includeDir, libraries: [...staged].toSorted() };
});

const writeManifest = Effect.fn('task.androidMpv.manifest')(function* (
  toolchain: AndroidToolchain,
  tools: Record<string, string>,
  libraries: readonly string[],
) {
  const artifacts: Record<string, { sha256: string }> = {};
  const collect = async (directory: string): Promise<void> => {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const full = path.join(directory, entry.name);
      if (entry.isDirectory()) {
        await collect(full);
      } else if (entry.isFile()) {
        const relative = path.relative(ANDROID_MPV_ARTIFACT_ROOT, full);
        const digest = createHash('sha256')
          .update(await readFile(full))
          .digest('hex');
        artifacts[relative] = { sha256: digest };
      }
    }
  };
  yield* io('manifest', () => collect(ANDROID_MPV_ARTIFACT_ROOT));
  const sources: Record<string, unknown> = {};
  for (const dep of pin.deps) {
    sources[dep.name] = {
      version: dep.version,
      source: dep.source,
      mesonOptions: dep.mesonOptions ?? null,
      configureOptions: dep.configureOptions ?? null,
      patch: dep.patch ?? null,
    };
  }
  const manifestPath = path.join(ANDROID_MPV_ARTIFACT_ROOT, 'manifest.json');
  yield* io('manifest', () =>
    writeFile(
      manifestPath,
      `${JSON.stringify(
        {
          schemaVersion: 1,
          abi: pin.abi,
          apiLevel: pin.apiLevel,
          ndk: {
            version: pin.ndkVersion,
            root: toolchain.ndkRoot,
            hostTag: toolchain.hostTag,
          },
          toolchain: {
            clangTriple: toolchain.clangTriple,
            llvmBinDir: toolchain.llvmBinDir,
            sysroot: toolchain.sysroot,
          },
          tools,
          sources,
          mpv: {
            source: pin.mpv.source,
            mesonOptions: pin.mpv.mesonOptions,
            patch: pin.mpv.patch ?? null,
          },
          libraries,
          artifacts,
          reproducibility: pin.reproducibility,
        },
        null,
        2,
      )}\n`,
    ),
  );
  return manifestPath;
});

export const buildAndroidMpv = Effect.fn('task.androidMpv.build')(function* (
  environment: Readonly<Record<string, string | undefined>>,
) {
  if (process.platform !== 'linux') {
    return yield* Effect.fail(
      new TaskAndroidMpvError({
        step: 'platform',
        message:
          'The Android libmpv pipeline requires Linux: it drives builds through GNU env -u/-C and a Linux NDK toolchain.',
      }),
    );
  }
  const toolchain: AndroidToolchain = yield* resolveAndroidToolchain(environment);
  if (
    toolchain.apiLevel !== pin.apiLevel ||
    toolchain.clangTriple !== `${toolchain.ndkTriple}${pin.apiLevel}`
  ) {
    return yield* Effect.fail(
      new TaskAndroidMpvError({
        step: 'toolchain',
        message: `Toolchain clang triple ${toolchain.clangTriple} does not match the pinned API level ${pin.apiLevel} for ${pin.abi}.`,
      }),
    );
  }
  if (pin.abi !== 'arm64-v8a' || toolchain.ndkTriple !== 'aarch64-linux-android') {
    return yield* Effect.fail(
      new TaskAndroidMpvError({
        step: 'toolchain',
        message: `Only arm64-v8a is staged; got clang triple ${toolchain.clangTriple}.`,
      }),
    );
  }
  const tools = yield* probeHostTools(toolchain);
  yield* io('workspace', async () => {
    await mkdir(SOURCE_ROOT, { recursive: true });
    await mkdir(CACHE_ROOT, { recursive: true });
    // The install prefix is rebuilt from scratch every run so artifacts always
    // come from the current pinned sources.
    await rm(PREFIX_DIR, { recursive: true, force: true });
    await mkdir(PREFIX_DIR, { recursive: true });
    // Flat install layout: DESTDIR installs land in <prefix>/usr/local which
    // must resolve to <prefix> itself (mpv-android does the same).
    await symlink('.', path.join(PREFIX_DIR, 'usr'));
    await symlink('.', path.join(PREFIX_DIR, 'local'));
  });
  yield* writeCrossfile(toolchain);

  const sourceDirs: Record<string, string> = {};
  for (const dep of pin.deps) {
    const pristineSource = yield* prepareSource(dep);
    if (dep.source.type === 'git') {
      yield* verifyGitSource(dep.name, pristineSource, dep.source.commit);
    }
    const sourceDir = yield* preparePatchedSource(dep, pristineSource);
    const expectedDiff =
      dep.patch === undefined
        ? ''
        : (yield* run('git', ['-C', sourceDir, ...SOURCE_DIFF_ARGS], {
            step: 'record source patch',
            buffered: true,
          })).stdout;
    sourceDirs[dep.name] = sourceDir;
    if (dep.name === 'mbedtls') {
      yield* buildMbedtls(sourceDir, toolchain);
    } else if (dep.name === 'ffmpeg') {
      yield* buildFfmpeg(dep, sourceDir, toolchain);
    } else if (dep.name === 'glslang') {
      yield* buildGlslang(sourceDir, toolchain);
    } else if (dep.mesonOptions !== undefined) {
      if (dep.name === 'libplacebo') {
        yield* stageVulkanHeaders(sourceDir);
        yield* mesonBuild(
          dep.name,
          sourceDir,
          [...dep.mesonOptions, `-Dvulkan-sdk=${PREFIX_DIR}`],
          toolchain,
        );
      } else {
        yield* mesonBuild(dep.name, sourceDir, dep.mesonOptions, toolchain);
      }
    } else {
      yield* configureBuild(dep, sourceDir, toolchain);
    }
    if (dep.source.type === 'git') {
      // Recheck after compilation to avoid certifying a checkout edited
      // during the build.
      yield* verifyGitSource(dep.name, sourceDir, dep.source.commit, expectedDiff);
    }
  }
  yield* fixLibplaceboPkgconfig();

  const pristineMpvSource = yield* fetchGitSource('mpv', pin.mpv.source);
  yield* verifyGitSource('mpv', pristineMpvSource, pin.mpv.source.commit);
  const mpvSource = yield* preparePatchedSource(
    { name: 'mpv', patch: pin.mpv.patch },
    pristineMpvSource,
  );
  const mpvDiff =
    pin.mpv.patch === undefined
      ? ''
      : (yield* run('git', ['-C', mpvSource, ...SOURCE_DIFF_ARGS], {
          step: 'record mpv source patch',
          buffered: true,
        })).stdout;
  sourceDirs.mpv = mpvSource;
  yield* verifyGitSource('mpv', mpvSource, pin.mpv.source.commit, mpvDiff);
  yield* mesonBuild('mpv', mpvSource, pin.mpv.mesonOptions, toolchain);
  yield* verifyGitSource('mpv', mpvSource, pin.mpv.source.commit, mpvDiff);
  const libmpvStatic = path.join(PREFIX_DIR, 'lib/libmpv.a');
  if (yield* io('libmpv artifact', () => exists(libmpvStatic))) {
    return yield* Effect.fail(
      new TaskAndroidMpvError({
        step: 'libmpv artifact',
        message: 'meson produced libmpv.a instead of libmpv.so; check the mpv meson options.',
      }),
    );
  }
  if (!(yield* io('libmpv artifact', () => exists(path.join(PREFIX_DIR, 'lib/libmpv.so'))))) {
    return yield* Effect.fail(
      new TaskAndroidMpvError({
        step: 'libmpv artifact',
        message: 'mpv build did not install lib/libmpv.so into the prefix.',
      }),
    );
  }

  const staged = yield* stageArtifacts(toolchain, sourceDirs);
  const manifestPath = yield* writeManifest(toolchain, tools, staged.libraries);
  yield* Effect.logInfo(
    `Android libmpv staged at ${ANDROID_MPV_ARTIFACT_ROOT}: ${staged.libraries.join(', ')}. ${pin.reproducibility}`,
  );
  const artifacts: AndroidMpvArtifacts = {
    artifactRoot: ANDROID_MPV_ARTIFACT_ROOT,
    includeDir: staged.includeDir,
    libDir: staged.libDir,
    libraries: staged.libraries,
    manifestPath,
  };
  return artifacts;
});
