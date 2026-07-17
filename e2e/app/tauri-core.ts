import { invoke as realInvoke } from '../../node_modules/@tauri-apps/api/core.js';
import { createControlledInvoke } from './fixture-registry';

export * from '../../node_modules/@tauri-apps/api/core.js';

export const invoke = createControlledInvoke(realInvoke);
