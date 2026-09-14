import { afterEach, describe, expect, test } from 'bun:test';
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';

import { Effect } from 'effect';

import { TaskMpvBuildError } from './errors';
import { parseWindowsPeImports, stageWindowsDependencies } from './windows-dependencies';

const temporaryDirectories: string[] = [];
afterEach(async () => {
  await Promise.all(
    temporaryDirectories.splice(0).map((directory) => rm(directory, { recursive: true })),
  );
});

const peDump = (imports: readonly string[], format = 'pei-x86-64') =>
  `fixture.dll:     file format ${format}\n\nThe Import Tables (interpreted .idata section contents)\n${imports
    .map((name) => `\tDLL Name: ${name}\n\tvma:  Hint/Ord Member-Name Bound-To\n`)
    .join('')}`;

async function fixture() {
  const root = await mkdtemp(path.join(tmpdir(), 'jellypilot-windows-dlls-'));
  temporaryDirectories.push(root);
  const runtime = path.join(root, 'runtime');
  const system = path.join(root, 'system');
  const destination = path.join(root, 'staging');
  await Promise.all([runtime, system, destination].map((directory) => mkdir(directory)));
  const library = path.join(root, 'libmpv-2.dll');
  const addDll = async (file: string, imports: readonly string[], format?: string) => {
    await writeFile(file, `binary payload for ${path.basename(file)}`);
    await writeFile(`${file}.dump`, peDump(imports, format));
  };
  // The only substituted boundary is objdump output; filesystem resolution and staging are real.
  const readPeDump = (file: string) =>
    Effect.tryPromise({
      try: () => readFile(`${file}.dump`, 'utf8'),
      catch: (cause) => new TaskMpvBuildError({ step: 'fixture objdump', message: String(cause) }),
    });
  const stage = () => stageWindowsDependencies(library, runtime, destination, system, readPeDump);
  return { root, runtime, system, destination, library, addDll, stage };
}

describe('Windows DLL closure staging', () => {
  test('copies transitive dependencies once through cycles and case differences, excluding system DLLs', async () => {
    const files = await fixture();
    await files.addDll(files.library, [
      'Codec.DLL',
      'KERNEL32.dll',
      'vulkan-1.dll',
      'api-ms-win-core-file-l1-1-0.dll',
    ]);
    await files.addDll(path.join(files.runtime, 'codec.dll'), ['HELPER.dll', 'kernel32.DLL']);
    await files.addDll(path.join(files.runtime, 'Helper.DLL'), [
      'CODEC.dll',
      'LIBMPV-2.DLL',
      'ext-ms-win-test.dll',
    ]);
    await writeFile(path.join(files.system, 'Kernel32.dll'), 'system DLL');
    await writeFile(path.join(files.system, 'vulkan-1.dll'), 'driver-installed Vulkan loader');
    await files.addDll(path.join(files.runtime, 'vulkan-1.dll'), ['kernel32.dll']);
    await files.addDll(path.join(files.runtime, 'unrelated.dll'), []);

    const result = await Effect.runPromise(files.stage());
    const stagedNames = await readdir(files.destination);
    expect(stagedNames.toSorted()).toEqual([
      'Helper.DLL',
      'codec.dll',
      'libmpv-2.dll',
      'vulkan-1.dll',
    ]);
    expect(await readFile(path.join(files.destination, 'vulkan-1.dll'), 'utf8')).toBe(
      'binary payload for vulkan-1.dll',
    );
    expect(result.systemDependencies).toEqual([
      'api-ms-win-core-file-l1-1-0.dll',
      'ext-ms-win-test.dll',
      'kernel32.dll',
    ]);
    expect(result.bundled).toHaveLength(4);
    for (const entry of result.bundled) {
      const stagedBytes = await readFile(path.join(files.destination, entry.name));
      expect(entry.sha256).toBe(createHash('sha256').update(stagedBytes).digest('hex'));
    }
  });

  test('missing transitive DLL fails before copying, even when an old staging file exists', async () => {
    const files = await fixture();
    await files.addDll(files.library, ['codec.dll']);
    await files.addDll(path.join(files.runtime, 'codec.dll'), ['missing.dll']);
    await writeFile(path.join(files.destination, 'missing.dll'), 'old staging file');
    const result = await Effect.runPromise(Effect.result(files.stage()));
    expect(result).toMatchObject({
      _tag: 'Failure',
      failure: {
        _tag: 'TaskMpvBuildError',
        message: expect.stringContaining('codec.dll imports missing.dll'),
      },
    });
    expect(await readdir(files.destination)).toEqual(['missing.dll']);
  });

  test('rejects a transitive x86 DLL without producing a partial bundle', async () => {
    const files = await fixture();
    await files.addDll(files.library, ['codec.dll']);
    await files.addDll(path.join(files.runtime, 'codec.dll'), [], 'pei-i386');
    const result = await Effect.runPromise(Effect.result(files.stage()));
    expect(result).toMatchObject({
      _tag: 'Failure',
      failure: { message: expect.stringContaining('expected a PE x86-64 DLL') },
    });
    expect(await readdir(files.destination)).toEqual([]);
  });

  test('rejects MSYS imports even if they are present in the supplied system directory', async () => {
    const files = await fixture();
    await files.addDll(files.library, ['msys-2.0.dll']);
    await writeFile(path.join(files.system, 'msys-2.0.dll'), 'must not treat as system');
    const result = await Effect.runPromise(Effect.result(files.stage()));
    expect(result).toMatchObject({
      _tag: 'Failure',
      failure: { message: expect.stringContaining('MSYS/Cygwin runtimes cannot be bundled') },
    });
    expect(await readdir(files.destination)).toEqual([]);
  });
});

describe('objdump PE imports', () => {
  test('reads DLL records with CRLF and duplicate names', async () => {
    const output = peDump(['KERNEL32.dll', 'D3D11.dll', 'kernel32.DLL']).replaceAll('\n', '\r\n');
    expect(await Effect.runPromise(parseWindowsPeImports('fixture.dll', output))).toEqual([
      'kernel32.dll',
      'd3d11.dll',
    ]);
  });

  test('fails closed for malformed DLL names or missing PE headers', async () => {
    for (const output of [peDump(['../escape.dll']), 'DLL Name: codec.dll\n']) {
      const parsed = parseWindowsPeImports('fixture.dll', output);
      const result = await Effect.runPromise(Effect.result(parsed));
      expect(result).toMatchObject({ _tag: 'Failure', failure: { _tag: 'TaskMpvBuildError' } });
    }
  });
});
