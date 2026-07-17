import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

import { Effect } from 'effect';

import { E2ePreflightError } from './errors';

const execFilePromise = promisify(execFile);

export interface ToolVersions {
  readonly bun: string;
  readonly node: string;
  readonly rust: string;
}

function versionParts(value: string): readonly number[] {
  const match = value.match(/(\d+)\.(\d+)(?:\.(\d+))?/);
  if (!match) return [];
  return [Number(match[1]), Number(match[2]), Number(match[3] ?? 0)];
}

export function versionAtLeast(value: string, minimum: readonly number[]): boolean {
  const current = versionParts(value);
  if (current.length === 0) return false;
  for (let index = 0; index < minimum.length; index += 1) {
    const actual = current[index] ?? 0;
    const required = minimum[index] ?? 0;
    if (actual !== required) return actual > required;
  }
  return true;
}

export function validateToolVersions(
  platform: NodeJS.Platform,
  versions: ToolVersions,
): string | undefined {
  if (platform !== 'linux') return 'Native E2E is supported only on Linux.';
  if (versions.bun !== '1.3.14') {
    return `Native E2E requires Bun 1.3.14; found ${versions.bun}.`;
  }
  if (!versionAtLeast(versions.node, [22, 11, 0])) {
    return `Native E2E requires Node 22.11 or newer; found ${versions.node}.`;
  }
  if (!versionAtLeast(versions.rust, [1, 85, 0])) {
    return `Native E2E requires Rust 1.85 or newer; found ${versions.rust}.`;
  }
  return undefined;
}

export const preflight = Effect.fn('e2e.preflight')(function* () {
  const versions = yield* Effect.tryPromise({
    try: async () => {
      const [bun, node, rust] = await Promise.all([
        execFilePromise('bun', ['--version']),
        execFilePromise('node', ['--version']),
        execFilePromise('rustc', ['--version']),
      ]);
      return {
        bun: bun.stdout.trim(),
        node: node.stdout.trim(),
        rust: rust.stdout.trim(),
      } satisfies ToolVersions;
    },
    catch: (cause) =>
      new E2ePreflightError({
        message: `Could not inspect E2E tools: ${cause instanceof Error ? cause.message : String(cause)}`,
      }),
  });

  const validationError = validateToolVersions(process.platform, versions);
  if (validationError) return yield* new E2ePreflightError({ message: validationError });

  return versions;
});
