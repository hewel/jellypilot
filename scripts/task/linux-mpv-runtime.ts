import { createHash } from 'node:crypto';
import {
  copyFile,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  realpath,
  rename,
  rm,
  stat,
  writeFile,
} from 'node:fs/promises';
import path from 'node:path';

import { Effect, Schema } from 'effect';

import libplaceboPin from '../../packaging/linux/libplacebo-source.json';
import { command } from './commands';
import { TaskMpvBuildError } from './errors';
import { buildMpv, MPV_ARTIFACT_ROOT } from './mpv';
import { runCommand } from './process';

const LIBRARY_RELATIVE = 'lib/jellypilot';
const LIBRARY_DIRECTORY = path.join(MPV_ARTIFACT_ROOT, LIBRARY_RELATIVE);
const NOTICE_RELATIVE = 'share/jellypilot/licenses/linux-runtime';
const NOTICE_DIRECTORY = path.join(MPV_ARTIFACT_ROOT, NOTICE_RELATIVE);
const ORIGIN_RPATH = '$ORIGIN';
const LINUX_SONAME = /^[a-z0-9_.+-]+\.so(?:\.[a-z0-9_+-]+)*$/i;

type PackageManager = 'dpkg' | 'pacman';
interface PackageOrigin {
  readonly manager: PackageManager;
  readonly packageName: string;
  readonly version: string;
}
interface NormalizedSystemDependency {
  readonly name: string;
  readonly originalPath: string;
  readonly origin: PackageOrigin;
}
interface RuntimeLibrary {
  readonly name: string;
  readonly origin:
    | { readonly type: 'pinned-source' }
    | { readonly type: 'distribution-package'; readonly origin: PackageOrigin };
  notices: string[];
}

export interface ElfDynamic {
  readonly needed: string[];
  readonly soname: string | null;
  readonly runpaths: string[];
}

export function parseElfDynamic(output: string): ElfDynamic {
  const needed = [...output.matchAll(/\(NEEDED\)\s+Shared library: \[(.*?)\]/g)].map(
    (match) => match[1] ?? '',
  );
  const soname = output.match(/\(SONAME\)\s+Library soname: \[(.*?)\]/)?.[1] ?? null;
  const runpaths = [
    ...output.matchAll(/\((?:RPATH|RUNPATH)\)\s+Library (?:rpath|runpath): \[(.*?)\]/g),
  ].map((match) => match[1] ?? '');
  return { needed, soname, runpaths };
}

export function parseLdconfig(output: string): Record<string, string> {
  const libraries: Record<string, string> = {};
  for (const line of output.split('\n')) {
    const match = line.match(/^\s*(\S+)\s+\(([^)]*)\)\s+=>\s+(\/.*)$/);
    if (match === null || !/x86[-_]64/.test(match[2] ?? '')) continue;
    const name = match[1];
    const libraryPath = match[3];
    if (
      name !== undefined &&
      libraryPath !== undefined &&
      !/(?:glibc-hwcaps|\/x86-64-v[2-4]\/)/.test(libraryPath) &&
      libraries[name] === undefined
    ) {
      libraries[name] = libraryPath;
    }
  }
  return libraries;
}

const SYSTEM_LIBRARY_REASONS: Record<string, 'glibc' | 'vulkan-loader'> = {
  'ld-linux-x86-64.so.2': 'glibc',
  'libanl.so.1': 'glibc',
  'libBrokenLocale.so.1': 'glibc',
  'libc.so.6': 'glibc',
  'libdl.so.2': 'glibc',
  'libm.so.6': 'glibc',
  'libmvec.so.1': 'glibc',
  'libnsl.so.1': 'glibc',
  'libnss_compat.so.2': 'glibc',
  'libnss_dns.so.2': 'glibc',
  'libnss_files.so.2': 'glibc',
  'libnss_hesiod.so.2': 'glibc',
  'libpthread.so.0': 'glibc',
  'libresolv.so.2': 'glibc',
  'librt.so.1': 'glibc',
  'libthread_db.so.1': 'glibc',
  'libutil.so.1': 'glibc',
  // Keep the distro Vulkan loader so it retains its normal ICD discovery policy.
  'libvulkan.so.1': 'vulkan-loader',
};

export function linuxSystemLibraryReason(name: string): 'glibc' | 'vulkan-loader' | null {
  return SYSTEM_LIBRARY_REASONS[name] ?? null;
}

const runtimeError = (step: string, message: string) =>
  new TaskMpvBuildError({ step: `Linux runtime ${step}`, message });

const io = <A>(step: string, operation: () => Promise<A>) =>
  Effect.tryPromise({
    try: operation,
    catch: (cause) => runtimeError(step, String(cause)),
  });

const run = (
  step: string,
  executable: string,
  args: readonly string[],
  options: {
    readonly env?: Readonly<Record<string, string>>;
    readonly buffered?: boolean;
    readonly acceptNonZero?: boolean;
  } = {},
) =>
  runCommand({
    ...command(executable, args, { ...options.env, LC_ALL: 'C' }),
    buffered: options.buffered ?? false,
    acceptNonZero: options.acceptNonZero ?? false,
  }).pipe(
    Effect.catchTag('TaskProcessError', (error) =>
      Effect.fail(runtimeError(step, `${error.message}\nCommand: ${error.command}`)),
    ),
  );

const fileExists = async (file: string) => {
  try {
    const info = await stat(file);
    return info.isFile();
  } catch {
    return false;
  }
};

const listFiles = async (root: string): Promise<string[]> => {
  const files: string[] = [];
  const visit = async (directory: string): Promise<void> => {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const full = path.join(directory, entry.name);
      if (entry.isDirectory()) await visit(full);
      else if (entry.isFile()) files.push(full);
      else throw new Error(`Unexpected non-regular runtime payload entry: ${full}`);
    }
  };
  await visit(root);
  return files.toSorted();
};

const verifyLibplaceboSource = Effect.fn('task.mpv.linuxRuntime.verifySource')(function* (
  checkout: string,
) {
  const result = yield* run('source revision', 'git', ['-C', checkout, 'rev-parse', 'HEAD'], {
    buffered: true,
  });
  const actual = result.stdout.trim();
  if (actual !== libplaceboPin.revision) {
    return yield* Effect.fail(
      runtimeError(
        'source revision',
        `libplacebo HEAD must be ${libplaceboPin.revision}; found ${actual}.`,
      ),
    );
  }
  const status = yield* run(
    'source cleanliness',
    'git',
    ['-C', checkout, 'status', '--porcelain', '--untracked-files=no', '--ignore-submodules=none'],
    { buffered: true },
  );
  if (status.stdout.trim() !== '') {
    return yield* Effect.fail(
      runtimeError(
        'source cleanliness',
        'Pinned libplacebo and its submodules must have no tracked changes.',
      ),
    );
  }
});

const detectPackageManager = Effect.fn('task.mpv.linuxRuntime.packageManager')(function* () {
  if (yield* io('package manager detection', () => fileExists('/usr/bin/dpkg-query'))) {
    return 'dpkg' as const;
  }
  if (yield* io('package manager detection', () => fileExists('/usr/bin/pacman'))) {
    return 'pacman' as const;
  }
  return yield* Effect.fail(
    runtimeError(
      'package manager detection',
      'Bundled system-library origins require dpkg-query or pacman.',
    ),
  );
});

const owningPackage = Effect.fn('task.mpv.linuxRuntime.owningPackage')(function* (
  manager: PackageManager,
  library: string,
) {
  const canonical = yield* io('library origin', () => realpath(library));
  let packageName: string | null = null;
  for (const candidate of new Set([library, canonical])) {
    const result = yield* run(
      'library origin',
      manager === 'dpkg' ? '/usr/bin/dpkg-query' : '/usr/bin/pacman',
      manager === 'dpkg' ? ['--search', candidate] : ['-Qoq', candidate],
      { buffered: true, acceptNonZero: true },
    );
    if (result.exitCode !== 0) continue;
    if (manager === 'pacman') {
      const candidateName = result.stdout.trim();
      if (/^[a-z0-9][a-z0-9@._+-]*$/i.test(candidateName)) {
        packageName = candidateName;
        break;
      }
      continue;
    }
    for (const line of result.stdout.split('\n')) {
      const match = line.match(/^([a-z0-9][a-z0-9+.-]+)(?::[a-z0-9]+)?:\s+\//i);
      if (match?.[1] !== undefined) {
        packageName = match[1];
        break;
      }
    }
    if (packageName !== null) break;
  }
  if (packageName === null) {
    return yield* Effect.fail(
      runtimeError(
        'library origin',
        `Cannot identify the distribution package and license origin for ${library}.`,
      ),
    );
  }
  const versionResult = yield* run(
    'library origin version',
    manager === 'dpkg' ? '/usr/bin/dpkg-query' : '/usr/bin/pacman',
    manager === 'dpkg' ? ['--show', `--showformat=\${Version}`, packageName] : ['-Q', packageName],
    { buffered: true },
  );
  const version =
    manager === 'dpkg'
      ? versionResult.stdout.trim()
      : versionResult.stdout.trim().slice(packageName.length).trim();
  if (version === '') {
    return yield* Effect.fail(
      runtimeError('library origin version', `No version recorded for ${packageName}.`),
    );
  }
  return { manager, packageName, version };
});

export const parsePacmanLicenseIds = (metadata: string): string[] => {
  const expression = metadata.match(/^Licenses[ \t]*:[ \t]*([^\n]*(?:\n[ \t]+[^\n]*)*)/m)?.[1];
  return [
    ...new Set(
      (expression ?? '')
        .replaceAll(/[()]/g, ' ')
        .split(/\s+/)
        .filter(
          (token) =>
            /^[A-Za-z0-9][A-Za-z0-9.+-]*$/.test(token) &&
            !['AND', 'OR', 'WITH', 'None'].includes(token),
        ),
    ),
  ];
};

const copyPackageNotices = Effect.fn('task.mpv.linuxRuntime.packageNotices')(function* (
  origin: PackageOrigin,
  noticeRoot: string,
) {
  const result = yield* run(
    'license collection',
    origin.manager === 'dpkg' ? '/usr/bin/dpkg-query' : '/usr/bin/pacman',
    origin.manager === 'dpkg' ? ['--listfiles', origin.packageName] : ['-Ql', origin.packageName],
    { buffered: true },
  );
  const prefix = origin.manager === 'dpkg' ? '/usr/share/doc/' : '/usr/share/licenses/';
  const candidates = result.stdout
    .split('\n')
    .map((line) =>
      origin.manager === 'dpkg' ? line.trim() : (line.match(/^\S+\s+(\/.*)$/)?.[1] ?? ''),
    )
    .filter((file) => {
      if (!file.startsWith(prefix) || file.endsWith('/')) return false;
      return origin.manager === 'pacman' || /\/copyright(?:\.gz)?$/.test(file);
    });
  if (origin.manager === 'pacman') {
    // Arch stores standard SPDX texts in the licenses package, not each library package.
    const metadata = yield* run(
      'license metadata',
      '/usr/bin/pacman',
      ['-Qi', origin.packageName],
      { buffered: true },
    );
    for (const id of parsePacmanLicenseIds(metadata.stdout)) {
      candidates.push(
        path.join(prefix, 'spdx', `${id}.txt`),
        path.join(prefix, 'spdx', 'exceptions', `${id}.txt`),
      );
    }
  }
  const notices: string[] = [];
  for (const source of [...new Set(candidates)].toSorted()) {
    if (!(yield* io('license collection', () => fileExists(source)))) continue;
    const relative = path.join(origin.manager, origin.packageName, source.slice(prefix.length));
    const destination = path.join(noticeRoot, relative);
    yield* io('license collection', async () => {
      await mkdir(path.dirname(destination), { recursive: true });
      await copyFile(source, destination);
    });
    notices.push(relative.split(path.sep).join('/'));
  }
  if (notices.length === 0) {
    return yield* Effect.fail(
      runtimeError(
        'license collection',
        `No ${origin.manager} license notice found for ${origin.packageName}.`,
      ),
    );
  }
  return notices;
});

const copyLibplaceboNotices = Effect.fn('task.mpv.linuxRuntime.sourceNotices')(function* (
  checkout: string,
  noticeRoot: string,
) {
  const sources = [
    'LICENSE',
    '3rdparty/Vulkan-Headers/LICENSE.md',
    '3rdparty/Vulkan-Headers/LICENSES/Apache-2.0.txt',
    '3rdparty/Vulkan-Headers/LICENSES/MIT.txt',
    '3rdparty/fast_float/LICENSE-APACHE',
    '3rdparty/fast_float/LICENSE-BOOST',
    '3rdparty/fast_float/LICENSE-MIT',
  ];
  const notices: string[] = [];
  for (const relative of sources) {
    const destinationRelative = path.join('libplacebo', relative);
    const destination = path.join(noticeRoot, destinationRelative);
    yield* io('libplacebo license collection', async () => {
      await mkdir(path.dirname(destination), { recursive: true });
      await copyFile(path.join(checkout, relative), destination);
    });
    notices.push(destinationRelative.split(path.sep).join('/'));
  }
  return notices;
});

const readElf = Effect.fn('task.mpv.linuxRuntime.readElf')(function* (library: string) {
  const result = yield* run('ELF inspection', 'readelf', ['--dynamic', '--wide', library], {
    buffered: true,
  });
  return parseElfDynamic(result.stdout);
});

const stageLibplaceboClosure = Effect.fn('task.mpv.linuxRuntime.closure')(function* (
  prefix: string,
  checkout: string,
  payload: string,
  noticeRoot: string,
  manager: PackageManager,
) {
  const ldconfig = yield* run('library index', 'ldconfig', ['-p'], { buffered: true });
  const systemLibraries = parseLdconfig(ldconfig.stdout);
  const privateLibrary = path.join(prefix, 'lib', libplaceboPin.soname);
  const initial = yield* readElf(privateLibrary);
  if (initial.soname !== libplaceboPin.soname) {
    return yield* Effect.fail(
      runtimeError(
        'libplacebo SONAME',
        `Expected ${libplaceboPin.soname}; found ${initial.soname ?? 'no SONAME'}.`,
      ),
    );
  }
  yield* io('libplacebo staging', () =>
    copyFile(privateLibrary, path.join(payload, libplaceboPin.soname)),
  );
  const libraries = new Map<string, RuntimeLibrary>([
    [
      libplaceboPin.soname,
      {
        name: libplaceboPin.soname,
        origin: { type: 'pinned-source' },
        notices: yield* copyLibplaceboNotices(checkout, noticeRoot),
      },
    ],
  ]);
  const retained = new Map<string, 'glibc' | 'vulkan-loader'>();
  const noticeCache = new Map<string, string[]>();
  // Walk only libplacebo's ELF closure, not mpv's existing distro dependency stack.
  // The retained Vulkan loader discovers GPU drivers dynamically; they are never copied.
  const queue = [privateLibrary];
  for (const importer of queue) {
    const dynamic = yield* readElf(importer);
    for (const name of dynamic.needed) {
      if (!LINUX_SONAME.test(name)) {
        return yield* Effect.fail(
          runtimeError('dependency validation', `${importer} has invalid NEEDED entry ${name}.`),
        );
      }
      const retainedReason = linuxSystemLibraryReason(name);
      if (retainedReason !== null) {
        retained.set(name, retainedReason);
        continue;
      }
      if (libraries.has(name)) continue;
      const privateCandidate = path.join(prefix, 'lib', name);
      const isPrivate = yield* io('dependency resolution', () => fileExists(privateCandidate));
      const source = isPrivate ? privateCandidate : systemLibraries[name];
      if (source === undefined) {
        return yield* Effect.fail(
          runtimeError('dependency resolution', `${importer} requires unavailable ${name}.`),
        );
      }
      if (isPrivate) {
        return yield* Effect.fail(
          runtimeError(
            'dependency resolution',
            `Unexpected additional private library ${name}; stage its pinned provenance explicitly.`,
          ),
        );
      }
      const origin = yield* owningPackage(manager, source);
      const cacheKey = `${origin.manager}:${origin.packageName}`;
      let notices = noticeCache.get(cacheKey);
      if (notices === undefined) {
        notices = yield* copyPackageNotices(origin, noticeRoot);
        noticeCache.set(cacheKey, notices);
      }
      yield* io('dependency staging', () => copyFile(source, path.join(payload, name)));
      libraries.set(name, {
        name,
        origin: { type: 'distribution-package', origin },
        notices,
      });
      queue.push(source);
    }
  }
  return { libraries: [...libraries.values()], retained };
});

const updateManifest = Effect.fn('task.mpv.linuxRuntime.manifest')(function* (
  libraries: RuntimeLibrary[],
  retained: ReadonlyMap<string, 'glibc' | 'vulkan-loader'>,
  tools: Readonly<Record<string, string>>,
  normalizedSystemDependencies: readonly NormalizedSystemDependency[],
) {
  const manifestPath = path.join(MPV_ARTIFACT_ROOT, 'manifest.json');
  const manifestText = yield* io('manifest read', () => readFile(manifestPath, 'utf8'));
  const value: unknown = yield* io('manifest parse', async () => JSON.parse(manifestText));
  const manifest = yield* Schema.decodeUnknownEffect(Schema.Record(Schema.String, Schema.Unknown))(
    value,
  ).pipe(
    Effect.catchTag('SchemaError', (error) =>
      Effect.fail(runtimeError('manifest shape', String(error))),
    ),
  );
  const decodedArtifacts = yield* Schema.decodeUnknownEffect(
    Schema.Record(Schema.String, Schema.Struct({ sha256: Schema.String })),
  )(manifest['artifacts']).pipe(
    Effect.catchTag('SchemaError', (error) =>
      Effect.fail(runtimeError('manifest artifacts', String(error))),
    ),
  );
  const artifacts = { ...decodedArtifacts };
  for (const key of Object.keys(artifacts)) {
    if (key.startsWith(`${LIBRARY_RELATIVE}/`) || key.startsWith(`${NOTICE_RELATIVE}/`)) {
      delete artifacts[key];
    }
  }
  const files = [
    ...(yield* io('manifest payload scan', () => listFiles(LIBRARY_DIRECTORY))),
    ...(yield* io('manifest notice scan', () => listFiles(NOTICE_DIRECTORY))),
  ];
  const hashes = new Map<string, string>();
  for (const file of files) {
    const relative = path.relative(MPV_ARTIFACT_ROOT, file).split(path.sep).join('/');
    const sha256 = createHash('sha256')
      .update(yield* io('manifest hashing', () => readFile(file)))
      .digest('hex');
    artifacts[relative] = { sha256 };
    hashes.set(relative, sha256);
  }
  const runtimeLibraries = libraries.toSorted((left, right) =>
    left.name.localeCompare(right.name, 'en'),
  );
  const libraryRecords: Record<string, unknown>[] = [];
  for (const library of runtimeLibraries) {
    const relative = `${LIBRARY_RELATIVE}/${library.name}`;
    const sha256 = hashes.get(relative);
    if (sha256 === undefined) {
      return yield* Effect.fail(
        runtimeError('manifest hashing', `Missing patched artifact hash for ${relative}.`),
      );
    }
    libraryRecords.push({
      name: library.name,
      sha256,
      origin:
        library.origin.type === 'pinned-source'
          ? library.origin
          : { type: library.origin.type, ...library.origin.origin },
      notices: library.notices.map((notice) => `${NOTICE_RELATIVE}/${notice}`),
    });
  }
  const linuxRuntime = {
    schemaVersion: 1,
    libplacebo: {
      remote: libplaceboPin.remote,
      tag: libplaceboPin.tag,
      revision: libplaceboPin.revision,
      version: libplaceboPin.version,
      soname: libplaceboPin.soname,
      mesonOptions: libplaceboPin.mesonOptions,
    },
    runpath: ORIGIN_RPATH,
    tools,
    libraries: libraryRecords,
    normalizedSystemDependencies,
    retainedSystemLibraries: [...retained]
      .map(([name, reason]) => ({ name, reason }))
      .toSorted((left, right) => left.name.localeCompare(right.name, 'en')),
    loaderCheck: { directDependency: libplaceboPin.soname, resolvedFrom: ORIGIN_RPATH },
  };
  yield* io('manifest write', () =>
    writeFile(
      manifestPath,
      `${JSON.stringify({ ...manifest, linuxRuntime, artifacts }, null, 2)}\n`,
    ),
  );
});

export const buildLinuxMpvRuntime = Effect.fn('task.mpv.linuxRuntime')(function* (
  source: string | null,
) {
  if (process.platform !== 'linux' || process.arch !== 'x64') {
    return yield* Effect.fail(
      runtimeError('platform', 'The private Linux runtime requires native x86_64 Linux.'),
    );
  }
  yield* io('artifact directory', () => mkdir(MPV_ARTIFACT_ROOT, { recursive: true }));
  const work = yield* io('work directory', () =>
    mkdtemp(path.join(MPV_ARTIFACT_ROOT, 'linux-runtime-')),
  );
  const tools: Record<string, string> = {};
  for (const executable of [
    'git',
    'meson',
    'ninja',
    'pkg-config',
    'cc',
    'c++',
    'python3',
    'readelf',
    'patchelf',
    'ldconfig',
    'ldd',
  ]) {
    const result = yield* run('prerequisite', executable, ['--version'], { buffered: true });
    tools[executable] = result.stdout.trim().split('\n')[0] ?? result.stderr.trim();
  }
  const manager = yield* detectPackageManager();
  const checkout = path.join(work, 'libplacebo-source');
  yield* run('source clone', 'git', [
    'clone',
    '--filter=blob:none',
    '--no-checkout',
    libplaceboPin.remote,
    checkout,
  ]);
  yield* run('source fetch', 'git', [
    '-C',
    checkout,
    'fetch',
    '--depth=1',
    'origin',
    libplaceboPin.revision,
  ]);
  yield* run('source checkout', 'git', [
    '-C',
    checkout,
    'checkout',
    '--detach',
    libplaceboPin.revision,
  ]);
  yield* run('submodule checkout', 'git', [
    '-C',
    checkout,
    'submodule',
    'update',
    '--init',
    '--recursive',
  ]);
  yield* verifyLibplaceboSource(checkout);
  const build = path.join(work, 'libplacebo-build');
  const prefix = path.join(work, 'prefix');
  yield* run('libplacebo configure', 'meson', [
    'setup',
    build,
    checkout,
    `--prefix=${prefix}`,
    '--libdir=lib',
    ...libplaceboPin.mesonOptions,
  ]);
  yield* run('libplacebo compile', 'meson', ['compile', '-C', build]);
  yield* run('libplacebo install', 'meson', ['install', '-C', build]);
  yield* verifyLibplaceboSource(checkout);
  const privatePkgConfig = path.join(prefix, 'lib/pkgconfig');
  const pkgConfigPath = [privatePkgConfig, process.env.PKG_CONFIG_PATH]
    .filter((value): value is string => value !== undefined && value !== '')
    .join(':');
  const environment = {
    PKG_CONFIG_PATH: pkgConfigPath,
    LD_LIBRARY_PATH: path.join(prefix, 'lib'),
  };
  const version = yield* run('libplacebo version', 'pkg-config', ['--modversion', 'libplacebo'], {
    buffered: true,
    env: environment,
  });
  if (version.stdout.trim() !== libplaceboPin.version) {
    return yield* Effect.fail(
      runtimeError(
        'libplacebo version',
        `Expected ${libplaceboPin.version}; pkg-config selected ${version.stdout.trim()}.`,
      ),
    );
  }
  yield* buildMpv(source, environment);

  const payload = path.join(work, 'payload');
  const noticeRoot = path.join(work, 'notices');
  yield* io('fresh payload', async () => {
    await mkdir(payload);
    await mkdir(noticeRoot);
    await copyFile(path.join(LIBRARY_DIRECTORY, 'libmpv.so'), path.join(payload, 'libmpv.so'));
  });
  const { libraries, retained } = yield* stageLibplaceboClosure(
    prefix,
    checkout,
    payload,
    noticeRoot,
    manager,
  );
  const payloadFiles = yield* io('payload scan', () => listFiles(payload));
  const normalizedSystemDependencies: NormalizedSystemDependency[] = [];
  for (const library of payloadFiles) {
    // Libraries without a SONAME can produce absolute NEEDED entries. Only normalize
    // distribution-owned system libraries; never hide a private build-prefix dependency.
    for (const dependency of (yield* readElf(library)).needed) {
      if (LINUX_SONAME.test(dependency)) continue;
      const name = path.basename(dependency);
      if (!/^(?:\/usr)?\/lib(?:64)?\//.test(dependency) || !LINUX_SONAME.test(name)) {
        return yield* Effect.fail(
          runtimeError(
            'dependency normalization',
            `Unsupported absolute dependency: ${dependency}.`,
          ),
        );
      }
      const origin = yield* owningPackage(manager, dependency);
      yield* run('dependency normalization', 'patchelf', [
        '--replace-needed',
        dependency,
        name,
        library,
      ]);
      normalizedSystemDependencies.push({ name, originalPath: dependency, origin });
    }
    yield* run('RPATH patching', 'patchelf', ['--set-rpath', ORIGIN_RPATH, library]);
  }
  yield* io('payload install', async () => {
    await rm(LIBRARY_DIRECTORY, { recursive: true, force: true });
    await rename(payload, LIBRARY_DIRECTORY);
    await rm(NOTICE_DIRECTORY, { recursive: true, force: true });
    await mkdir(path.dirname(NOTICE_DIRECTORY), { recursive: true });
    await rename(noticeRoot, NOTICE_DIRECTORY);
  });
  for (const library of yield* io('RPATH verification scan', () => listFiles(LIBRARY_DIRECTORY))) {
    const dynamic = yield* readElf(library);
    if (dynamic.runpaths.length !== 1 || dynamic.runpaths[0] !== ORIGIN_RPATH) {
      return yield* Effect.fail(
        runtimeError(
          'RPATH verification',
          `${library} must have only ${ORIGIN_RPATH}; found ${dynamic.runpaths.join(', ')}.`,
        ),
      );
    }
    for (const name of dynamic.needed) {
      if (!LINUX_SONAME.test(name)) {
        return yield* Effect.fail(
          runtimeError('dependency verification', `${library} has invalid NEEDED ${name}.`),
        );
      }
    }
  }
  // Remove caller overrides so this checks the staged RPATHs, not a development prefix.
  const loader = yield* run('loader verification', 'env', [
    '-u',
    'LD_LIBRARY_PATH',
    'ldd',
    path.join(LIBRARY_DIRECTORY, 'libmpv.so'),
  ]);
  if (loader.stdout.includes('not found') || loader.stderr.includes('not found')) {
    return yield* Effect.fail(
      runtimeError('loader verification', `${loader.stdout}\n${loader.stderr}`),
    );
  }
  const expected = `${libplaceboPin.soname} => ${path.join(
    LIBRARY_DIRECTORY,
    libplaceboPin.soname,
  )}`;
  if (!loader.stdout.includes(expected)) {
    return yield* Effect.fail(
      runtimeError('loader verification', `Expected ${expected}; found:\n${loader.stdout}`),
    );
  }
  yield* updateManifest(libraries, retained, tools, normalizedSystemDependencies);
  yield* Effect.logInfo(
    `Linux mpv runtime staged with private libplacebo ${libplaceboPin.version} at ${LIBRARY_DIRECTORY}.`,
  );
});
