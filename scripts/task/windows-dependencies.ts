import { createHash } from 'node:crypto';
import { copyFile, readdir, readFile } from 'node:fs/promises';
import path from 'node:path';

import { Effect } from 'effect';

import { TaskMpvBuildError } from './errors';
import { runCommand } from './process';

const dependencyError = (message: string) =>
  new TaskMpvBuildError({ step: 'Windows DLL dependencies', message });

/** Parse the DLL Name records emitted by GNU objdump. */
export const parseWindowsPeImports = Effect.fn('task.mpv.parseWindowsPeImports')(function* (
  libraryPath: string,
  output: string,
) {
  if (!/\bfile format pei-x86-64\s*$/m.test(output)) {
    return yield* Effect.fail(dependencyError(`${libraryPath}: expected a PE x86-64 DLL.`));
  }
  const imports = new Set<string>();
  for (const match of output.matchAll(/^\s*DLL Name:\s*(.*?)\s*$/gm)) {
    const name = match[1] ?? '';
    if (!/^[a-z\d_.+ -]+\.dll$/i.test(name)) {
      return yield* Effect.fail(
        dependencyError(`${libraryPath}: invalid imported DLL name ${JSON.stringify(name)}.`),
      );
    }
    imports.add(name.toLowerCase());
  }
  return [...imports];
});

const dllIndex = Effect.fn('task.mpv.windowsDllIndex')(function* (directory: string) {
  const entries = yield* Effect.tryPromise({
    try: () => readdir(directory, { withFileTypes: true }),
    catch: (cause) => dependencyError(`Cannot list ${directory}: ${String(cause)}`),
  });
  const index = new Map<string, string>();
  for (const entry of entries) {
    if (!entry.isFile() || !entry.name.toLowerCase().endsWith('.dll')) continue;
    const normalized = entry.name.toLowerCase();
    if (index.has(normalized)) {
      return yield* Effect.fail(
        dependencyError(`Ambiguous case-insensitive DLL name ${normalized} in ${directory}.`),
      );
    }
    index.set(normalized, entry.name);
  }
  return index;
});

type ReadPeDump = (libraryPath: string) => Effect.Effect<string, TaskMpvBuildError>;

// Large FFmpeg DLL export tables exceed the task runner's bounded output capture.
// Filter the subprocess stream before capture, retaining the PE header and imports.
const peImportFilter = `import subprocess, sys
with subprocess.Popen([sys.argv[1], '-p', sys.argv[2]], stdout=subprocess.PIPE, text=True, encoding='utf-8', errors='replace') as child:
    for line in child.stdout:
        if 'file format ' in line or line.lstrip().startswith('DLL Name:'):
            sys.stdout.write(line)
    sys.exit(child.wait())
`;

export const readWindowsPeDump = Effect.fn('task.mpv.readWindowsPeDump')(
  (runtimeDirectory: string, libraryPath: string) =>
    runCommand({
      command: path.join(runtimeDirectory, 'python.exe'),
      args: ['-c', peImportFilter, path.join(runtimeDirectory, 'objdump.exe'), libraryPath],
      buffered: true,
      env: { LC_ALL: 'C' },
    }).pipe(
      Effect.map((result) => result.stdout),
      Effect.catchTag('TaskProcessError', (error) =>
        Effect.fail(dependencyError(`Cannot inspect ${libraryPath}: ${error.message}`)),
      ),
    ),
);

/** Resolve the entire native DLL closure before copying into a caller-owned fresh directory. */
export const stageWindowsDependencies = Effect.fn('task.mpv.stageWindowsDependencies')(function* (
  libraryPath: string,
  runtimeDirectory: string,
  destinationDirectory: string,
  systemDirectory: string,
  readPeDump: ReadPeDump = (file) => readWindowsPeDump(runtimeDirectory, file),
) {
  const runtimeDlls = yield* dllIndex(runtimeDirectory);
  const systemDlls = yield* dllIndex(systemDirectory);
  const mainName = path.basename(libraryPath);
  const resolved = new Map([[mainName.toLowerCase(), libraryPath]]);
  const queue = [libraryPath];
  const systemDependencies = new Set<string>();

  for (const importer of queue) {
    const dump = yield* readPeDump(importer);
    const imports = yield* parseWindowsPeImports(importer, dump);
    for (const name of imports) {
      if (/^(msys-|cyg)/i.test(name)) {
        return yield* Effect.fail(
          dependencyError(`${importer} imports ${name}; MSYS/Cygwin runtimes cannot be bundled.`),
        );
      }
      // A driver-installed DLL in System32 must not hide the explicit runtime's copy.
      if (
        /^(api-ms-win-|ext-ms-win-)/i.test(name) ||
        (systemDlls.has(name) && !runtimeDlls.has(name))
      ) {
        systemDependencies.add(name);
        continue;
      }
      if (resolved.has(name)) continue;
      const runtimeName = yield* Effect.fromNullishOr(runtimeDlls.get(name)).pipe(
        Effect.catchTag('NoSuchElementError', () =>
          Effect.fail(
            dependencyError(`${importer} imports ${name}, missing from ${runtimeDirectory}.`),
          ),
        ),
      );
      const dependencyPath = path.join(runtimeDirectory, runtimeName);
      resolved.set(name, dependencyPath);
      queue.push(dependencyPath);
    }
  }

  const bundled: { name: string; sha256: string }[] = [];
  for (const source of [...resolved.values()].toSorted()) {
    const name = path.basename(source);
    const destination = path.join(destinationDirectory, name);
    const sha256 = yield* Effect.tryPromise({
      try: async () => {
        await copyFile(source, destination);
        return createHash('sha256')
          .update(await readFile(destination))
          .digest('hex');
      },
      catch: (cause) => dependencyError(`Cannot stage ${source}: ${String(cause)}`),
    });
    bundled.push({ name, sha256 });
  }
  return { bundled, systemDependencies: [...systemDependencies].toSorted() };
});
