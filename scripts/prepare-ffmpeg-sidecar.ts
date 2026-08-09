#!/usr/bin/env bun
import { createHash } from 'node:crypto';
import { existsSync } from 'node:fs';
import { chmod, mkdir, readFile, rename, unlink, writeFile } from 'node:fs/promises';
import { arch, platform } from 'node:os';
import path from 'node:path';

import { Cause, Effect, Exit, Option, Schema } from 'effect';

const repositoryRoot = path.resolve(import.meta.dirname, '..');
const manifestPath = path.join(repositoryRoot, 'packaging/ffmpeg/manifest.json');
const packageManifestPath = path.join(repositoryRoot, 'node_modules/ffmpeg-static/package.json');
const outputDirectory = path.join(repositoryRoot, 'src-tauri/binaries');

const FileAssetSchema = Schema.Struct({
  name: Schema.String,
  sha256: Schema.String,
});

const TargetAssetsSchema = Schema.Struct({
  binary: FileAssetSchema,
  license: FileAssetSchema,
  buildInfo: FileAssetSchema,
});

const ManifestSchema = Schema.Struct({
  schemaVersion: Schema.Literal(1),
  ffmpegStaticVersion: Schema.String,
  ffmpegStaticSourceCommit: Schema.String,
  binaryReleaseTag: Schema.String,
  binaryBaseUrl: Schema.String,
  assets: Schema.Record(Schema.String, TargetAssetsSchema),
});

const FfmpegStaticPackageSchema = Schema.Struct({
  name: Schema.Literal('ffmpeg-static'),
  version: Schema.String,
  'ffmpeg-static': Schema.Struct({
    'binary-release-tag': Schema.String,
  }),
});

class FfmpegSidecarError extends Schema.TaggedErrorClass<FfmpegSidecarError>()(
  'FfmpegSidecarError',
  {
    message: Schema.String,
    cause: Schema.optionalKey(Schema.Defect()),
  },
) {}

function sidecarError(message: string, cause?: unknown): FfmpegSidecarError {
  return cause === undefined
    ? FfmpegSidecarError.make({ message })
    : FfmpegSidecarError.make({ message, cause });
}

const readBytes = Effect.fn('ffmpeg.readBytes')((filePath: string) =>
  Effect.tryPromise({
    try: () => readFile(filePath),
    catch: (cause) => sidecarError(`Could not read ${filePath}.`, cause),
  }),
);

const readJson = Effect.fn('ffmpeg.readJson')((filePath: string) =>
  Effect.tryPromise({
    try: async (): Promise<unknown> => JSON.parse(await readFile(filePath, 'utf8')),
    catch: (cause) => sidecarError(`Could not parse ${filePath}.`, cause),
  }),
);

function sha256(contents: Uint8Array): string {
  return createHash('sha256').update(contents).digest('hex');
}

const readOptionalBytes = Effect.fn('ffmpeg.readOptionalBytes')(function* (filePath: string) {
  if (!existsSync(filePath)) return Option.none<Uint8Array>();
  return Option.some(yield* readBytes(filePath));
});

const parseCli = Effect.fn('ffmpeg.parseCli')(function* (args: readonly string[]) {
  let target = Option.none<string>();
  let verifyOnly = false;

  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument === '--verify') {
      verifyOnly = true;
      continue;
    }
    if (argument === '--target') {
      const value = args[index + 1];
      if (value === undefined) {
        return yield* sidecarError('`--target` requires a Rust target triple.');
      }
      target = Option.some(value);
      index += 1;
      continue;
    }
    return yield* sidecarError(
      `Unknown FFmpeg sidecar option: ${argument}\n` +
        'Usage: bun run ffmpeg:prepare [--verify] [--target <rust-target-triple>]',
    );
  }

  return { target, verifyOnly };
});

const nativeTargets: Readonly<Record<string, string>> = {
  'darwin-arm64': 'aarch64-apple-darwin',
  'darwin-x64': 'x86_64-apple-darwin',
  'linux-arm64': 'aarch64-unknown-linux-gnu',
  'linux-x64': 'x86_64-unknown-linux-gnu',
  'win32-x64': 'x86_64-pc-windows-msvc',
};

const tauriPlatforms: Readonly<Record<string, string>> = {
  darwin: 'darwin',
  linux: 'linux',
  windows: 'win32',
  win32: 'win32',
};

const tauriArchitectures: Readonly<Record<string, string>> = {
  aarch64: 'arm64',
  arm64: 'arm64',
  x64: 'x64',
  x86_64: 'x64',
};

function inferredTarget(): Option.Option<string> {
  const tauriPlatform = process.env.TAURI_ENV_PLATFORM;
  const tauriArchitecture = process.env.TAURI_ENV_ARCH;
  if (tauriPlatform !== undefined && tauriArchitecture !== undefined) {
    const normalizedPlatform = tauriPlatforms[tauriPlatform];
    const normalizedArchitecture = tauriArchitectures[tauriArchitecture];
    if (normalizedPlatform !== undefined && normalizedArchitecture !== undefined) {
      return Option.fromNullishOr(nativeTargets[`${normalizedPlatform}-${normalizedArchitecture}`]);
    }
  }
  return Option.fromNullishOr(nativeTargets[`${platform()}-${arch()}`]);
}

const download = Effect.fn('ffmpeg.download')(function* (url: string) {
  const response = yield* Effect.tryPromise({
    try: () => fetch(url, { redirect: 'follow' }),
    catch: (cause) => sidecarError(`Could not download ${url}.`, cause),
  });
  if (!response.ok) {
    return yield* sidecarError(
      `Could not download ${url}: ${response.status} ${response.statusText}.`,
    );
  }
  return yield* Effect.tryPromise({
    try: async () => new Uint8Array(await response.arrayBuffer()),
    catch: (cause) => sidecarError(`Could not read the response from ${url}.`, cause),
  });
});

const writeAtomically = Effect.fn('ffmpeg.writeAtomically')(
  (filePath: string, contents: Uint8Array, executable: boolean) =>
    Effect.acquireUseRelease(
      Effect.tryPromise({
        try: async () => {
          await mkdir(path.dirname(filePath), { recursive: true });
          return `${filePath}.${process.pid}.tmp`;
        },
        catch: (cause) => sidecarError(`Could not prepare ${filePath}.`, cause),
      }),
      (temporaryPath) =>
        Effect.tryPromise({
          try: async () => {
            await writeFile(temporaryPath, contents, { mode: executable ? 0o755 : 0o644 });
            if (executable) await chmod(temporaryPath, 0o755);
            await rename(temporaryPath, filePath);
          },
          catch: (cause) => sidecarError(`Could not write ${filePath}.`, cause),
        }),
      (temporaryPath) =>
        Effect.tryPromise({
          try: () => unlink(temporaryPath),
          catch: (cause) => sidecarError(`Could not remove ${temporaryPath}.`, cause),
        }).pipe(Effect.orElseSucceed(() => undefined)),
    ),
);

interface MaterializeOptions {
  readonly asset: typeof FileAssetSchema.Type;
  readonly executable: boolean;
  readonly localPackagePath: Option.Option<string>;
  readonly outputPath: string;
  readonly sourceDirectory: Option.Option<string>;
  readonly verifyOnly: boolean;
}

const materialize = Effect.fn('ffmpeg.materialize')(function* (
  manifestBaseUrl: string,
  options: MaterializeOptions,
) {
  const existing = yield* readOptionalBytes(options.outputPath);
  if (Option.isSome(existing) && sha256(existing.value) === options.asset.sha256) {
    if (options.executable) {
      yield* Effect.tryPromise({
        try: () => chmod(options.outputPath, 0o755),
        catch: (cause) => sidecarError(`Could not mark ${options.outputPath} executable.`, cause),
      });
    }
    return;
  }

  if (options.verifyOnly) {
    const state = Option.isSome(existing) ? 'has the wrong checksum' : 'is missing';
    return yield* sidecarError(`${options.outputPath} ${state}. Run \`bun run ffmpeg:prepare\`.`);
  }

  const sourcePath = Option.match(options.sourceDirectory, {
    onNone: () => options.localPackagePath,
    onSome: (directory) => Option.some(path.join(directory, options.asset.name)),
  });

  const contents = yield* Option.match(sourcePath, {
    onNone: () => download(`${manifestBaseUrl}/${options.asset.name}`),
    onSome: (filePath) => readBytes(filePath),
  });
  const actualChecksum = sha256(contents);
  if (actualChecksum !== options.asset.sha256) {
    return yield* sidecarError(
      `Checksum mismatch for ${options.asset.name}: expected ${options.asset.sha256}, found ${actualChecksum}.`,
    );
  }

  yield* writeAtomically(options.outputPath, contents, options.executable);
});

const validatedPackageAsset = Effect.fn('ffmpeg.validatedPackageAsset')(function* (
  filePath: string,
  expectedChecksum: string,
) {
  const contents = yield* readOptionalBytes(filePath);
  if (Option.isNone(contents)) return Option.none<string>();
  const actualChecksum = sha256(contents.value);
  if (actualChecksum !== expectedChecksum) {
    return yield* sidecarError(
      `Installed ffmpeg-static file ${filePath} has checksum ${actualChecksum}; ` +
        `expected ${expectedChecksum}. Reinstall dependencies with scripts disabled and rerun ` +
        'the sidecar preparation.',
    );
  }
  return Option.some(filePath);
});

const program = Effect.gen(function* () {
  const options = yield* parseCli(process.argv.slice(2));
  const manifestJson = yield* readJson(manifestPath);
  const manifest = yield* Schema.decodeUnknownEffect(ManifestSchema)(manifestJson).pipe(
    Effect.mapError((cause) => sidecarError(`Invalid FFmpeg manifest: ${String(cause)}`, cause)),
  );
  const packageJson = yield* readJson(packageManifestPath);
  const ffmpegPackage = yield* Schema.decodeUnknownEffect(FfmpegStaticPackageSchema)(
    packageJson,
  ).pipe(
    Effect.mapError((cause) =>
      sidecarError(
        'ffmpeg-static is not installed correctly. Run `bun install --ignore-scripts`.',
        cause,
      ),
    ),
  );

  if (
    ffmpegPackage.version !== manifest.ffmpegStaticVersion ||
    ffmpegPackage['ffmpeg-static']['binary-release-tag'] !== manifest.binaryReleaseTag
  ) {
    return yield* sidecarError(
      `Expected ffmpeg-static ${manifest.ffmpegStaticVersion} / ${manifest.binaryReleaseTag}, ` +
        `found ${ffmpegPackage.version} / ${ffmpegPackage['ffmpeg-static']['binary-release-tag']}.`,
    );
  }

  const target = yield* Option.match(options.target.pipe(Option.orElse(inferredTarget)), {
    onNone: () =>
      Effect.fail(
        sidecarError(
          `Unsupported FFmpeg host ${platform()}-${arch()}. Pass a supported Rust target with --target.`,
        ),
      ),
    onSome: Effect.succeed,
  });
  const assets = manifest.assets[target];
  if (assets === undefined) {
    return yield* sidecarError(
      `FFmpeg sidecars are not available for ${target}. Supported targets: ${Object.keys(
        manifest.assets,
      ).join(', ')}.`,
    );
  }

  const sourceDirectory = Option.fromNullishOr(process.env.JELLYPILOT_FFMPEG_ASSET_DIR);
  const nativeTarget = Option.getOrUndefined(inferredTarget());
  const useInstalledPackage = target === nativeTarget && Option.isNone(sourceDirectory);
  const installedBinaryName = platform() === 'win32' ? 'ffmpeg.exe' : 'ffmpeg';
  const installedBinaryPath = path.join(
    repositoryRoot,
    'node_modules/ffmpeg-static',
    installedBinaryName,
  );
  const packagePaths = useInstalledPackage
    ? {
        binary: yield* validatedPackageAsset(installedBinaryPath, assets.binary.sha256),
        license: yield* validatedPackageAsset(
          `${installedBinaryPath}.LICENSE`,
          assets.license.sha256,
        ),
        buildInfo: yield* validatedPackageAsset(
          `${installedBinaryPath}.README`,
          assets.buildInfo.sha256,
        ),
      }
    : {
        binary: Option.none<string>(),
        license: Option.none<string>(),
        buildInfo: Option.none<string>(),
      };

  const executableExtension = target.includes('windows') ? '.exe' : '';
  yield* materialize(manifest.binaryBaseUrl, {
    asset: assets.binary,
    executable: true,
    localPackagePath: packagePaths.binary,
    outputPath: path.join(outputDirectory, `ffmpeg-${target}${executableExtension}`),
    sourceDirectory,
    verifyOnly: options.verifyOnly,
  });
  yield* materialize(manifest.binaryBaseUrl, {
    asset: assets.license,
    executable: false,
    localPackagePath: packagePaths.license,
    outputPath: path.join(outputDirectory, 'ffmpeg.LICENSE.txt'),
    sourceDirectory,
    verifyOnly: options.verifyOnly,
  });
  yield* materialize(manifest.binaryBaseUrl, {
    asset: assets.buildInfo,
    executable: false,
    localPackagePath: packagePaths.buildInfo,
    outputPath: path.join(outputDirectory, 'ffmpeg.BUILD-INFO.txt'),
    sourceDirectory,
    verifyOnly: options.verifyOnly,
  });

  console.info(`${options.verifyOnly ? 'Verified' : 'Prepared'} FFmpeg sidecar for ${target}.`);
});

const exit = await Effect.runPromiseExit(program);
if (Exit.isFailure(exit)) {
  console.error(Cause.pretty(exit.cause));
  process.exitCode = 1;
}
