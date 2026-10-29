import { describe, expect, test } from 'bun:test';

import { parseElfDynamic, parseLdconfig, parsePacmanLicenseIds } from './linux-mpv-runtime';

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
