import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';

async function filesUnder(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = await Promise.all(
    entries.map((entry) => {
      const path = join(directory, entry.name);
      return entry.isDirectory() ? filesUnder(path) : [path];
    }),
  );
  return files.flat();
}

const tauriConfig = JSON.parse(await readFile('src-tauri/tauri.conf.json', 'utf8'));
if (tauriConfig.app?.withGlobalTauri === true) {
  throw new Error('Production Tauri configuration exposes the global Tauri bridge');
}

const defaultCapability = await readFile('src-tauri/capabilities/default.json', 'utf8');
if (/webdriver|wdio/i.test(defaultCapability)) {
  throw new Error('Production capabilities include WebDriver permissions');
}

const cargoManifest = await readFile('src-tauri/Cargo.toml', 'utf8');
for (const dependency of ['tauri-plugin-wdio', 'tauri-plugin-wdio-webdriver']) {
  const declaration = new RegExp(`^${dependency}\\s*=\\s*\\{[^\\n]*optional\\s*=\\s*true`, 'm');
  if (!declaration.test(cargoManifest)) {
    throw new Error(`${dependency} must remain optional`);
  }
}

const rustSource = await readFile('src-tauri/src/lib.rs', 'utf8');
if (!rustSource.includes('all(feature = "webdriver", not(debug_assertions))')) {
  throw new Error('Release compile guard for the WebDriver feature is missing');
}

const distFiles = await filesUnder('dist');
const productionFiles = distFiles.filter((file) => /\.(?:css|html|js)$/.test(file));
for (const file of productionFiles) {
  const source = await readFile(file, 'utf8');
  if (/@wdio|webdriverio|PUBLIC_WEBDRIVER|__TAURI__\s*=/.test(source)) {
    throw new Error(`Production asset contains a WebDriver surface: ${file}`);
  }
}

console.log('Production assets, capabilities, globals, and Cargo boundary exclude WebDriver');
