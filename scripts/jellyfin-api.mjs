import { spawn } from 'node:child_process';
import { readFile, rm, writeFile } from 'node:fs/promises';

const STABLE_SPEC_URL =
  'https://api.jellyfin.org/openapi/jellyfin-openapi-stable.json';
const SNAPSHOT_PATH = 'src-tauri/openapi/jellyfin-openapi-stable.json';
const CONFIG_PATH = 'src-tauri/openapi/jellyfin-api-generator.json';
const OUTPUT_DIR = 'src-tauri/jellyfin-api';

const command = process.argv[2];

async function updateSnapshot() {
  const response = await fetch(STABLE_SPEC_URL, {
    headers: { 'User-Agent': 'jmsr-openapi-snapshot' },
  });

  if (!response.ok) {
    throw new Error(
      `Failed to fetch Jellyfin OpenAPI stable spec: ${response.status} ${response.statusText}`,
    );
  }

  await writeFile(SNAPSHOT_PATH, await response.text());
}

async function patchGeneratedClient() {
  const path = `${OUTPUT_DIR}/src/models/transcoding_info.rs`;
  const source = await readFile(path, 'utf8');
  const patched = source.replace(
    `pub enum TranscodeReasons {
}

impl Default for TranscodeReasons {
    fn default() -> TranscodeReasons {
        Self::
    }
}`,
    `pub enum TranscodeReasons {
    #[serde(other)]
    Unknown,
}

impl Default for TranscodeReasons {
    fn default() -> TranscodeReasons {
        Self::Unknown
    }
}`,
  );

  if (patched === source) {
    throw new Error('Generated TranscodeReasons patch did not apply');
  }

  await writeFile(path, patched);
}

async function runGenerator() {
  await rm(OUTPUT_DIR, { force: true, recursive: true });

  await new Promise((resolve, reject) => {
    const child = spawn(
      'openapi-generator-cli',
      [
        'generate',
        '-i',
        SNAPSHOT_PATH,
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

  await patchGeneratedClient();
}

if (command === 'generate') {
  await runGenerator();
} else if (command === 'update') {
  await updateSnapshot();
  await runGenerator();
} else {
  console.error('Usage: bun scripts/jellyfin-api.mjs <generate|update>');
  process.exitCode = 1;
}
