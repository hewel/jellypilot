import { spawn } from 'node:child_process';
import { readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const EMBY_SDK_TAG = '4.9.3.0';
const SPEC_URL = `https://raw.githubusercontent.com/MediaBrowser/Emby.SDK/${EMBY_SDK_TAG}/Resources/OpenApi/openapi_v3.json`;
const SNAPSHOT_PATH = 'src-tauri/openapi/emby-openapi-4.9.3.0.json';
const CONFIG_PATH = 'src-tauri/openapi/emby-api-generator.json';
const OUTPUT_DIR = 'src-tauri/emby-api';
const PATCHED_SPEC_PATH = join(tmpdir(), 'jellypilot-emby-openapi-4.9.3.0-generator.json');

const command = process.argv[2];

async function updateSnapshot() {
  const response = await fetch(SPEC_URL, {
    headers: { 'User-Agent': 'jellypilot-emby-openapi-snapshot' },
  });

  if (!response.ok) {
    throw new Error(`Failed to fetch Emby OpenAPI spec: ${response.status} ${response.statusText}`);
  }

  await writeFile(SNAPSHOT_PATH, await response.text());
}

async function createPatchedGeneratorSpec() {
  const spec = JSON.parse(await readFile(SNAPSHOT_PATH, 'utf8'));
  const imagePath = spec.paths?.['/Users/{Id}/Images/{Type}/{Index}'];
  const indexParameter = imagePath?.delete?.parameters?.find(
    (parameter) => parameter?.name === 'Index' && parameter?.in === 'path',
  );
  const postParameters = imagePath?.post?.parameters;

  if (!indexParameter || !Array.isArray(postParameters)) {
    throw new Error('Emby OpenAPI image path patch target was not found');
  }

  if (postParameters.some((parameter) => parameter?.name === 'Index' && parameter?.in === 'path')) {
    throw new Error('Emby OpenAPI image path patch already applied upstream');
  }

  postParameters.push(indexParameter);
  await writeFile(PATCHED_SPEC_PATH, `${JSON.stringify(spec, null, 2)}\n`);
  return PATCHED_SPEC_PATH;
}

async function runGenerator() {
  await rm(OUTPUT_DIR, { force: true, recursive: true });
  const generatorSpecPath = await createPatchedGeneratorSpec();

  try {
    await new Promise((resolve, reject) => {
      const child = spawn(
        'openapi-generator-cli',
        [
          'generate',
          '-i',
          generatorSpecPath,
          '-g',
          'rust',
          '-o',
          OUTPUT_DIR,
          '-c',
          CONFIG_PATH,
          '--global-property',
          'apiTests=false,modelTests=false,apiDocs=false,modelDocs=false',
        ],
        { stdio: 'inherit' },
      );

      child.on('error', reject);
      child.on('exit', (code, signal) => {
        if (code === 0) {
          resolve();
          return;
        }

        reject(new Error(`openapi-generator-cli exited with ${signal ?? code}`));
      });
    });
  } finally {
    await rm(generatorSpecPath, { force: true });
  }
}

if (command === 'generate') {
  await runGenerator();
} else if (command === 'update') {
  await updateSnapshot();
  await runGenerator();
} else {
  console.error('Usage: bun scripts/emby-api.mjs <generate|update>');
  process.exitCode = 1;
}
