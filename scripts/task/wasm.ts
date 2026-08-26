import { Effect, Match } from 'effect';

import { command, wasmBuildCommand } from './commands';
import { runCommand } from './process';

export type WasmTask =
  | { readonly action: 'install' }
  | { readonly action: 'build'; readonly mode?: '--dev' | '--release' };

export const runWasm = Effect.fn('task.wasm')((task: WasmTask) =>
  Match.value(task).pipe(
    Match.when({ action: 'install' }, () =>
      runCommand(
        command('cargo', ['install', 'wasm-pack', '--version', '0.15.0', '--locked']),
      ).pipe(Effect.asVoid),
    ),
    Match.when({ action: 'build' }, ({ mode }) =>
      runCommand(wasmBuildCommand(mode)).pipe(Effect.asVoid),
    ),
    Match.exhaustive,
  ),
);
