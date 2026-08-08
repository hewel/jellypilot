import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';

import { beforeAll, expect, test } from '@rstest/core';

import init, {
  LibraryBrowseCore,
  type LibraryBrowseEvent,
} from '../crates/jellypilot-core-wasm/pkg/jellypilot_core_wasm.js';

beforeAll(async () => {
  const bytes = await readFile(
    resolve('crates/jellypilot-core-wasm/pkg/jellypilot_core_wasm_bg.wasm'),
  );
  await init({ module_or_path: Uint8Array.from(bytes).buffer });
});

test('malformed JavaScript input does not poison the stateful WASM handle', () => {
  const core = new LibraryBrowseCore();

  expect(() =>
    core.dispatch({ tag: 'configure', enabled: true } as unknown as LibraryBrowseEvent),
  ).toThrow(/invalid library browse event/i);

  const update = core.dispatch({ tag: 'configure', sourceId: 'movies', enabled: true });
  expect(update.snapshot.status).toEqual({ tag: 'loading' });
  expect(update.commands.map((command) => command.tag)).toEqual(['resetViewport', 'loadPage']);
  expect(() => core.free()).not.toThrow();
});
