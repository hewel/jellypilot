import { readFileSync } from 'node:fs';
import { expect, test } from '@rstest/core';

const readText = (path: string) => readFileSync(path, 'utf8');

test('release workflow builds a source-based Arch package with desktop integration', () => {
  const pkgbuild = readText('packaging/arch/PKGBUILD');
  const desktopEntry = readText('packaging/arch/top.pigfun.jmsr.desktop');
  const releaseWorkflow = readText('.github/workflows/release.yml');

  expect(pkgbuild).toContain('pkgname=jmsr');
  expect(pkgbuild).toContain('pkgver=1.4.0');
  expect(pkgbuild).toContain("options=('!lto')");
  expect(pkgbuild).toContain(
    '"git+https://github.com/hewel/jmsr.git#tag=v$pkgver"',
  );
  expect(pkgbuild).toContain("'top.pigfun.jmsr.desktop'");
  expect(pkgbuild).toContain("'SKIP'");
  expect(pkgbuild).toContain(
    "'fad7c84a15d92cefa357b4e2cf2c0877e0a97ae8da2114468d9f8c86a27bc98f'",
  );
  expect(pkgbuild).toContain("'mpv'");
  expect(pkgbuild).toContain('bun tauri build --no-bundle --ci');
  expect(pkgbuild).toContain('install -Dm755 "src-tauri/target/release/jmsr"');
  expect(pkgbuild).toContain(
    'install -Dm644 "$srcdir/top.pigfun.jmsr.desktop"',
  );
  expect(pkgbuild).not.toContain('install=');

  expect(desktopEntry).toContain('Name=JMSR');
  expect(desktopEntry).toContain('Exec=jmsr');
  expect(desktopEntry).toContain('Icon=top.pigfun.jmsr');
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
