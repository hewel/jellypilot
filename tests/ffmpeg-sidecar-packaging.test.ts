import { readFileSync } from 'node:fs';

import { expect, test } from '@rstest/core';
import { Schema } from 'effect';

const readText = (filePath: string) => readFileSync(filePath, 'utf8');

const FileAssetSchema = Schema.Struct({
  name: Schema.String,
  sha256: Schema.String.check(Schema.isPattern(/^[0-9a-f]{64}$/)),
});

const TargetAssetsSchema = Schema.Struct({
  ffmpeg: FileAssetSchema,
  ffprobe: FileAssetSchema,
  license: FileAssetSchema,
  buildInfo: FileAssetSchema,
});

const ManifestSchema = Schema.Struct({
  schemaVersion: Schema.Literal(2),
  binaryReleaseTag: Schema.Literal('b6.1.1'),
  binaryBaseUrl: Schema.String,
  assets: Schema.Record(Schema.String, TargetAssetsSchema),
});

const TauriConfigSchema = Schema.Struct({
  bundle: Schema.Struct({
    externalBin: Schema.Array(Schema.String),
  }),
});

const supportedTargets = [
  'aarch64-apple-darwin',
  'x86_64-apple-darwin',
  'aarch64-unknown-linux-gnu',
  'x86_64-unknown-linux-gnu',
  'x86_64-pc-windows-msvc',
];

test('the sidecar manifest pins a paired FFmpeg and FFprobe asset for every target', () => {
  const manifest = Schema.decodeUnknownSync(ManifestSchema)(
    JSON.parse(readText('packaging/ffmpeg/manifest.json')),
  );

  expect(manifest.binaryBaseUrl).toBe(
    'https://github.com/eugeneware/ffmpeg-static/releases/download/b6.1.1',
  );
  expect(Object.keys(manifest.assets).toSorted()).toEqual(supportedTargets.toSorted());

  for (const target of supportedTargets) {
    const assets = manifest.assets[target];
    expect(assets).toBeDefined();
    expect(assets?.ffmpeg.name).toMatch(/^ffmpeg-/);
    expect(assets?.ffprobe.name).toMatch(/^ffprobe-/);
    expect(assets?.ffmpeg.name.replace('ffmpeg-', '')).toBe(
      assets?.ffprobe.name.replace('ffprobe-', ''),
    );
  }
});

test('Tauri and Arch packaging include both verified sidecars', () => {
  const tauriConfig = Schema.decodeUnknownSync(TauriConfigSchema)(
    JSON.parse(readText('src-tauri/tauri.conf.json')),
  );
  const pkgbuild = readText('packaging/arch/PKGBUILD');

  expect(tauriConfig.bundle.externalBin).toEqual(['binaries/ffmpeg', 'binaries/ffprobe']);
  expect(pkgbuild).toContain(
    'https://github.com/eugeneware/ffmpeg-static/releases/download/b6.1.1/ffprobe-linux-x64',
  );
  expect(pkgbuild).toContain('4f231a1960d83e403d08f7971e271707bec278a9ae18e21b8b5b03186668450d');
  expect(pkgbuild).toContain('src-tauri/binaries/ffprobe-x86_64-unknown-linux-gnu');
  expect(pkgbuild).toContain('$pkgdir/usr/lib/$pkgname/ffprobe');
});

test('the owned FFmpeg workflow pins source and build-system revisions', () => {
  const workflow = readText('.github/workflows/ffmpeg-sidecars.yml');

  expect(workflow).toContain('FFMPEG_TAG_COMMIT: d32b387f2b0a484599d4587d651891f0c63c4238');
  expect(workflow).toContain('BTBN_COMMIT: 2437e7b868da3c11872367b15f3c613b87c24819');
  expect(workflow).toContain('./makeimage.sh linux64 gpl 9.0');
  expect(workflow).toContain('./build.sh linux64 gpl 9.0');
  expect(workflow).toContain('git -C ffbuild/ffmpeg rev-parse HEAD');
  expect(workflow).toContain("grep -F -- '--enable-nonfree'");
  expect(workflow).toContain('immutable-releases');
  expect(workflow).toContain('PROVENANCE.json');
  expect(workflow).toContain('SHA256SUMS');
});
