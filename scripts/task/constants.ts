import path from 'node:path';

export const REPO_ROOT = path.resolve(import.meta.dirname, '../..');
export const MAX_CAPTURED_OUTPUT_BYTES = 2_000_000;
