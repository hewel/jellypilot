import { readFileSync } from 'node:fs';

import { expect, test } from '@rstest/core';

const readText = (path: string) => readFileSync(path, 'utf8');

test('release workflow builds a source-based Arch package with desktop integration', () => {
  const packageJson = JSON.parse(readText('package.json')) as { version: string };
  const pkgbuild = readText('packaging/arch/PKGBUILD');
  const desktopEntry = readText('packaging/arch/top.pigfun.jellypilot.desktop');
  const releaseWorkflow = readText('.github/workflows/release.yml');

  const pkgverPattern = new RegExp(
    `^${packageJson.version.replaceAll('.', String.raw`\.`)}(rc\\d+)?$`,
  );
  expect(pkgbuild.match(/^pkgver=(.+)$/m)?.[1]).toMatch(pkgverPattern);
  expect(pkgbuild).toContain('pkgname=jellypilot');
  expect(pkgbuild).toContain("options=('!lto')");
  expect(pkgbuild).toContain('"git+https://github.com/hewel/jellypilot.git#tag=v$pkgver"');
  expect(pkgbuild).toContain("'top.pigfun.jellypilot.desktop'");
  expect(pkgbuild).toContain("'mpv'");
  expect(pkgbuild).toContain('bun tauri build --no-bundle --ci');
  expect(pkgbuild).toContain('install -Dm755 "src-tauri/target/release/jellypilot"');
  expect(pkgbuild).toContain('install -Dm644 "$srcdir/top.pigfun.jellypilot.desktop"');

  expect(desktopEntry).toContain('Name=JellyPilot');
  expect(desktopEntry).toContain('Exec=jellypilot');
  expect(desktopEntry).toContain('Icon=top.pigfun.jellypilot');
  expect(desktopEntry).toContain('Categories=AudioVideo;Player;');

  expect(releaseWorkflow).toContain('arch-package:');
  expect(releaseWorkflow).toContain('makepkg --syncdeps --noconfirm');
  expect(releaseWorkflow).toContain('needs: [changelog, build, arch-package]');
});
