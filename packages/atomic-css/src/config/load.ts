import { readFileSync } from 'node:fs'
import { pathToFileURL } from 'node:url'
import type { AtomicConfig, LoadedConfig } from './types.js'

const APPROVED_IMPORTS = new Set([
  '@jellypilot/atomic-css',
  '@jellypilot/atomic-css/preset-mini',
  '@jellypilot/atomic-css/config',
])

const PROHIBITED_FORMS = [
  { re: /\brequire\s*\(/, name: 'require-call' },
  { re: /\bimport\s*\(/, name: 'dynamic-import' },
  { re: /\beval\s*\(/, name: 'eval-call' },
  { re: /\bFunction\s*\(/, name: 'function-constructor' },
  { re: /\bprocess\b/, name: 'process-access' },
  { re: /\bfs\b/, name: 'fs-access' },
  { re: /\blet\b/, name: 'mutable-let' },
  { re: /\bvar\b/, name: 'mutable-var' },
] as const

/** Load atomic.config.ts with static prohibition checks before evaluation. */
export async function loadAtomicConfig(configPath: string): Promise<LoadedConfig> {
  const source = readFileSync(configPath, 'utf8')
  const diagnostics = collectConfigDiagnostics(source)
  if (diagnostics.length > 0) {
    throw new Error(
      `atomic-css config diagnostics:\n${diagnostics.map((d) => `  - ${d}`).join('\n')}`,
    )
  }

  const mod: unknown = await import(`${pathToFileURL(configPath).href}?t=${Date.now()}`)
  const config = extractDefaultConfig(mod)
  return { config, sourcePath: configPath }
}

export function collectConfigDiagnostics(source: string): string[] {
  const diagnostics: string[] = []
  for (const form of PROHIBITED_FORMS) {
    if (form.re.test(source)) {
      diagnostics.push(`${form.name}: prohibited execution form`)
    }
  }
  for (const match of source.matchAll(/from\s+['"]([^'"]+)['"]/g)) {
    const spec = match[1]
    if (spec === undefined) continue
    if (spec.startsWith('.') || spec.startsWith('/')) {
      diagnostics.push(`relative-import: ${spec} is not an approved package import`)
      continue
    }
    if (!APPROVED_IMPORTS.has(spec)) {
      diagnostics.push(`unapproved-import: ${spec}`)
    }
  }
  return diagnostics
}

function extractDefaultConfig(mod: unknown): AtomicConfig {
  if (!mod || typeof mod !== 'object') {
    throw new Error('atomic-css config must export a default config object')
  }
  const candidate =
    'default' in mod ? (mod as { default: unknown }).default : mod
  if (!candidate || typeof candidate !== 'object') {
    throw new Error('atomic-css config default export must be an object')
  }
  if (!('preset' in candidate) || candidate.preset !== 'preset-mini') {
    throw new Error('atomic-css config.preset must be "preset-mini"')
  }
  return candidate as AtomicConfig
}
