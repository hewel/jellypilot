import { readFileSync } from 'node:fs';

import { expect, test } from '@rstest/core';

const readText = (path: string) => readFileSync(path, 'utf8');

test('release workflow builds a source-based Arch package with desktop integration', () => {
  const pkgbuild = readText('packaging/arch/PKGBUILD');
  const desktopEntry = readText('packaging/arch/top.pigfun.jellypilot.desktop');
  const releaseWorkflow = readText('.github/workflows/release.yml');

  expect(pkgbuild).toContain('pkgname=jellypilot');
  expect(pkgbuild).toContain('pkgver=1.4.1');
  expect(pkgbuild).toContain("options=('!lto')");
  expect(pkgbuild).toContain('"git+https://github.com/hewel/jellypilot.git#tag=v$pkgver"');
  expect(pkgbuild).toContain("'top.pigfun.jellypilot.desktop'");
  expect(pkgbuild).toContain("'SKIP'");
  expect(pkgbuild).toContain("'7236e1197fe9cd03f7df541f77a9710f4cf9a8c1f7de6df3a6f7def6e60d7651'");
  expect(pkgbuild).toContain("'mpv'");
  expect(pkgbuild).toContain('bun tauri build --no-bundle --ci');
  expect(pkgbuild).toContain('install -Dm755 "src-tauri/target/release/jellypilot"');
  expect(pkgbuild).toContain('install -Dm644 "$srcdir/top.pigfun.jellypilot.desktop"');
  expect(pkgbuild).not.toContain('install=');

  expect(desktopEntry).toContain('Name=JellyPilot');
  expect(desktopEntry).toContain('Exec=jellypilot');
  expect(desktopEntry).toContain('Icon=top.pigfun.jellypilot');
  expect(desktopEntry).toContain('Categories=AudioVideo;Player;');

  expect(releaseWorkflow).toContain('arch-package:');
  expect(releaseWorkflow).toContain('container: archlinux:base-devel');
  expect(releaseWorkflow).toContain('nodejs git bun rust');
  expect(releaseWorkflow).toContain('makepkg --syncdeps --noconfirm');
  expect(releaseWorkflow).toContain('needs: [changelog, build, arch-package]');
  expect(releaseWorkflow).toContain('name: arch-artifacts');
  expect(releaseWorkflow).toContain('path: artifacts');
  expect(releaseWorkflow).toContain('*.pkg.tar.zst');
});
