import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { familyRegistry } from '../src/registry/index'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const barrel = readFileSync(join(root, 'src/index.ts'), 'utf8')

let failed = false
for (const entry of familyRegistry) {
  const exportPattern = new RegExp(
    `export\\s*\\{[^}]*\\b${entry.exportName}\\b[^}]*\\}`,
  )
  if (!exportPattern.test(barrel)) {
    console.error(`registry drift: barrel missing export ${entry.exportName}`)
    failed = true
  }
  try {
    readFileSync(join(root, 'src', entry.path.replace('./', '')), 'utf8')
  } catch {
    console.error(`registry drift: missing component file ${entry.path}`)
    failed = true
  }
}

if (failed) process.exit(1)
console.log(`registry ok (${familyRegistry.length} families)`)
