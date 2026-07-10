import { createHash } from 'node:crypto'
import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import type { AtomicDeclaration } from './extract.js'

export type ManifestEntry = {
  id: string
  kind: 'rule' | 'composition'
  declarations: AtomicDeclaration[]
  origins: string[]
  digest: string
}

export type ProjectManifest = {
  version: '0.1.0'
  entries: ManifestEntry[]
}

export function canonicalRuleId(declarations: AtomicDeclaration[]): string {
  const payload = declarations
    .map((declaration) => `${declaration.property}:${declaration.value}`)
    .sort()
    .join('|')
  return sha(`rule|${payload}`)
}

export function canonicalCompositionId(
  ruleIds: string[],
): string {
  return sha(`composition|${[...ruleIds].sort().join('|')}`)
}

export function buildManifest(
  entries: Array<{
    declarations: AtomicDeclaration[]
    origin: string
  }>,
): ProjectManifest {
  const byId = new Map<string, ManifestEntry>()
  for (const entry of entries) {
    const id = canonicalRuleId(entry.declarations)
    const existing = byId.get(id)
    if (existing) {
      if (!existing.origins.includes(entry.origin)) {
        existing.origins.push(entry.origin)
        existing.origins.sort()
      }
      continue
    }
    byId.set(id, {
      id,
      kind: 'rule',
      declarations: [...entry.declarations].sort((a, b) =>
        a.property.localeCompare(b.property),
      ),
      origins: [entry.origin],
      digest: sha(JSON.stringify(entry.declarations)),
    })
  }
  return {
    version: '0.1.0',
    entries: [...byId.values()].sort((a, b) => a.id.localeCompare(b.id)),
  }
}

export function publishManifest(
  outDir: string,
  manifest: ProjectManifest,
): string {
  mkdirSync(outDir, { recursive: true })
  const path = join(outDir, 'manifest.json')
  writeFileSync(path, `${JSON.stringify(manifest, null, 2)}\n`, 'utf8')
  return path
}

function sha(value: string): string {
  return createHash('sha256').update(value).digest('hex').slice(0, 16)
}
