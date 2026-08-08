import type {
  InitInput,
  LibraryBrowseCore,
} from '../../crates/jellypilot-core-wasm/pkg/jellypilot_core_wasm.js';
import type * as LibraryBrowseWasmModule from '../../crates/jellypilot-core-wasm/pkg/jellypilot_core_wasm.js';

export type {
  LibraryBrowseCacheMode,
  LibraryBrowseCommand,
  LibraryBrowseEvent,
  LibraryBrowseLoadToken,
  LibraryBrowseSnapshot,
  LibraryBrowseStatus,
  LibraryBrowseUpdate,
} from '../../crates/jellypilot-core-wasm/pkg/jellypilot_core_wasm.js';

let initializedModule: Promise<typeof LibraryBrowseWasmModule> | undefined;
let initializationInput: InitInput | undefined;
let initializationFailureMessage: string | undefined;

async function initializeLibraryBrowseWasm(): Promise<typeof LibraryBrowseWasmModule> {
  if (initializationFailureMessage) {
    throw new Error(initializationFailureMessage);
  }
  const wasm = await import('../../crates/jellypilot-core-wasm/pkg/jellypilot_core_wasm.js');
  await (initializationInput === undefined
    ? wasm.default()
    : wasm.default({ module_or_path: initializationInput }));
  return wasm;
}

function loadLibraryBrowseWasm(): Promise<typeof LibraryBrowseWasmModule> {
  const pending = initializedModule ?? initializeLibraryBrowseWasm();
  initializedModule = pending;
  void pending.catch(() => {
    if (initializedModule === pending) {
      initializedModule = undefined;
    }
  });
  return pending;
}

/** Lazily initializes the shared WASM module and creates route-local browse state. */
export async function loadLibraryBrowseCore(): Promise<LibraryBrowseCore> {
  const wasm = await loadLibraryBrowseWasm();
  return new wasm.LibraryBrowseCore();
}

/** Supplies WASM bytes in test runtimes that cannot fetch generated file URLs. */
export function setLibraryBrowseWasmInitInputForTests(input: InitInput | undefined): void {
  initializationInput = input;
  initializedModule = undefined;
}

/** Injects a deterministic lazy-initialization failure for adapter tests. */
export function setLibraryBrowseWasmInitializationFailureForTests(
  failure: Error | undefined,
): void {
  initializationFailureMessage = failure?.message;
  initializedModule = undefined;
}
