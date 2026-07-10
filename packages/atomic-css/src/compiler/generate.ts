import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import type { AtomicDeclaration } from './extract.js'

export type GeneratedTransport = 'real-file'

export type GenerateOptions = {
  outDir: string
  transport?: GeneratedTransport
}

/** Emit layer + registry modules used as the parent-entry prepend. */
export function ensureGeneratedBaseline(options: GenerateOptions): {
  registryPath: string
  layerPath: string
} {
  const transport = options.transport ?? 'real-file'
  if (transport !== 'real-file') {
    throw new Error('virtual transport is not released; real-file is the baseline')
  }
  mkdirSync(options.outDir, { recursive: true })
  const layerPath = join(options.outDir, 'layers.css.ts')
  const registryPath = join(options.outDir, 'registry.css.ts')
  writeFileSync(
    layerPath,
    `import { layer } from '@vanilla-extract/css';\n\nexport const preflightLayer = layer('atomic.preflight');\nexport const utilitiesLayer = layer('atomic.utilities');\n`,
    'utf8',
  )
  writeFileSync(
    registryPath,
    `import './layers.css.ts';\n// atomic registry: parent entries import this once for deterministic order\nexport {};\n`,
    'utf8',
  )
  return { registryPath, layerPath }
}

export function ruleModuleSource(
  id: string,
  declarations: AtomicDeclaration[],
): string {
  const styleObject = Object.fromEntries(
    declarations.map((declaration) => [declaration.property, declaration.value]),
  )
  return `import { style } from '@vanilla-extract/css';\nimport { utilitiesLayer } from './layers.css.ts';\n\nexport const rule_${id} = style({\n  '@layer': utilitiesLayer,\n  ...${JSON.stringify(styleObject, null, 2)},\n});\n`
}
