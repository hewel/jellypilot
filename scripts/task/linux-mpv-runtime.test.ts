import { afterEach, describe, expect, test } from 'bun:test';
import { mkdir, mkdtemp, readFile, readdir, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { gzipSync } from 'node:zlib';

import { Effect } from 'effect';

import {
  copyPackageNoticeFiles,
  parseElfDynamic,
  parseLdconfig,
  parsePacmanLicenseIds,
} from './linux-mpv-runtime';

const temporaryDirectories: string[] = [];
afterEach(async () => {
  await Promise.all(
    temporaryDirectories.splice(0).map((directory) => rm(directory, { recursive: true })),
  );
});

async function noticeFixture() {
  const root = await mkdtemp(path.join(tmpdir(), 'jellypilot-linux-notices-'));
  temporaryDirectories.push(root);
  const documentation = path.join(root, 'doc');
  const destination = path.join(root, 'notices');
  await Promise.all([documentation, destination].map((directory) => mkdir(directory)));
  const collect = (listedCandidates: readonly string[] = []) =>
    copyPackageNoticeFiles(
      { manager: 'dpkg', packageName: 'libstdc++6', version: '16.1.0' },
      listedCandidates,
      documentation,
      destination,
    );
  return { documentation, destination, collect };
}

describe('Linux mpv runtime ELF metadata', () => {
  test('parses direct dependencies, SONAME and all lookup paths', () => {
    const dynamic = parseElfDynamic(`
0x0000000000000001 (NEEDED) Shared library: [libplacebo.so.387]
0x0000000000000001 (NEEDED) Shared library: [libvulkan.so.1]
0x000000000000000f (RPATH) Library rpath: [/tmp/private/lib]
0x000000000000001d (RUNPATH) Library runpath: [$ORIGIN]
0x000000000000000e (SONAME) Library soname: [libmpv.so]
`);
    expect(dynamic).toEqual({
      needed: ['libplacebo.so.387', 'libvulkan.so.1'],
      soname: 'libmpv.so',
      runpaths: ['/tmp/private/lib', '$ORIGIN'],
    });
  });

  test('indexes generic native loader entries in resolution order', () => {
    expect(
      parseLdconfig(`
	libplacebo.so.387 (libc6,x86-64) => /opt/private/lib/libplacebo.so.387
	libplacebo.so.387 (libc6,x86-64) => /opt/duplicate/lib/libplacebo.so.387
	libplacebo.so.387 (libc6) => /lib/i386-linux-gnu/libplacebo.so.387
	libshaderc_shared.so.1 (libc6,x86-64) => /usr/lib/glibc-hwcaps/x86-64-v3/libshaderc_shared.so.1
	libshaderc_shared.so.1 (libc6,x86-64) => /usr/lib/libshaderc_shared.so.1
`),
    ).toEqual({
      'libplacebo.so.387': '/opt/private/lib/libplacebo.so.387',
      'libshaderc_shared.so.1': '/usr/lib/libshaderc_shared.so.1',
    });
  });

  test('collects shared license texts and exceptions from wrapped pacman expressions', () => {
    expect(
      parsePacmanLicenseIds(`Name            : libgcc
Licenses        : (GPL-3.0-or-later WITH GCC-exception-3.1)
                  AND GFDL-1.3-or-later OR GPL-3.0-or-later
Groups          : None
`),
    ).toEqual(['GPL-3.0-or-later', 'GCC-exception-3.1', 'GFDL-1.3-or-later']);
    expect(parsePacmanLicenseIds('Licenses : ../../private\n')).toEqual([]);
  });
});

describe('Linux runtime package notices', () => {
  test.each(['copyright', 'copyright.gz'])(
    'copies %s through a documentation directory symlink absent from the file listing',
    async (filename) => {
      const files = await noticeFixture();
      const target = path.join(files.documentation, 'gcc-16-base');
      await mkdir(target);
      const text = Buffer.from('GCC runtime library license and exception\n');
      const contents = filename.endsWith('.gz') ? gzipSync(text) : text;
      await writeFile(path.join(target, filename), contents);
      await symlink(
        process.platform === 'win32' ? target : 'gcc-16-base',
        path.join(files.documentation, 'libstdc++6'),
        process.platform === 'win32' ? 'junction' : 'dir',
      );

      const notices = await Effect.runPromise(files.collect());
      const relative = `dpkg/libstdc++6/libstdc++6/${filename}`;
      expect(notices).toEqual([relative]);
      expect(await readFile(path.join(files.destination, relative))).toEqual(contents);
    },
  );

  test('retains listed notices and does not duplicate the standard copyright', async () => {
    const files = await noticeFixture();
    const packageDoc = path.join(files.documentation, 'libstdc++6');
    const extraDoc = path.join(packageDoc, 'additional');
    await mkdir(extraDoc, { recursive: true });
    const standard = path.join(packageDoc, 'copyright');
    const extra = path.join(extraDoc, 'copyright');
    await writeFile(standard, 'main license');
    await writeFile(extra, 'additional license');

    const notices = await Effect.runPromise(files.collect([standard, extra, standard]));
    expect(notices).toEqual([
      'dpkg/libstdc++6/libstdc++6/additional/copyright',
      'dpkg/libstdc++6/libstdc++6/copyright',
    ]);
    expect(await readFile(path.join(files.destination, notices[0]!), 'utf8')).toBe(
      'additional license',
    );
    expect(await readFile(path.join(files.destination, notices[1]!), 'utf8')).toBe('main license');
  });

  test.each([false, true])('fails for a missing notice (broken link: %s)', async (brokenLink) => {
    const files = await noticeFixture();
    const unrelated = path.join(files.documentation, 'unrelated');
    await mkdir(unrelated);
    await writeFile(path.join(unrelated, 'copyright'), 'not this package');
    if (brokenLink) {
      await symlink(
        path.join(files.documentation, 'missing'),
        path.join(files.documentation, 'libstdc++6'),
        process.platform === 'win32' ? 'junction' : 'dir',
      );
    }

    const result = await Effect.runPromise(Effect.result(files.collect()));
    expect(result).toMatchObject({
      _tag: 'Failure',
      failure: { _tag: 'TaskMpvBuildError', step: 'Linux runtime license collection' },
    });
    expect(await readdir(files.destination)).toEqual([]);
  });
});
