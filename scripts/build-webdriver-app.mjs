import { cp, rm } from 'node:fs/promises';
import { resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const fixturePath = resolve(root, 'tests/e2e/fixtures/webdriver-capability.json');
const capabilityPath = resolve(root, 'src-tauri/capabilities/webdriver.json');

await cp(fixturePath, capabilityPath);

try {
  const child = Bun.spawn(
    [
      'bun',
      'tauri',
      'build',
      '--debug',
      '--no-bundle',
      '--features',
      'webdriver',
      '--config',
      'src-tauri/tauri.webdriver.conf.json',
    ],
    {
      cwd: root,
      env: { ...Bun.env, CI: 'true', PUBLIC_WEBDRIVER: '1' },
      stderr: 'inherit',
      stdout: 'inherit',
    },
  );

  const exitCode = await child.exited;
  if (exitCode !== 0) {
    process.exit(exitCode);
  }
} finally {
  await rm(capabilityPath, { force: true });
}
