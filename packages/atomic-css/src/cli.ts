#!/usr/bin/env node
import { existsSync } from 'node:fs'
import { resolve } from 'node:path'
import { loadAtomicConfig } from './config/load.js'
import {
  checkGeneratedTypes,
  defaultGeneratedTypesPath,
  writeGeneratedTypes,
} from './compiler/types-gen.js'
import { buildAtomicSchema } from './schema/schema.js'

const args = process.argv.slice(2)
const command = args[0] ?? 'help'
const root = process.cwd()
const configFlagIndex = args.indexOf('--config')
const configPath = resolve(
  root,
  configFlagIndex >= 0 && args[configFlagIndex + 1]
    ? args[configFlagIndex + 1]!
    : 'atomic.config.ts',
)

async function main(): Promise<void> {
  if (command === 'generate') {
    const schema = await schemaFromConfig(configPath)
    const outFile = defaultGeneratedTypesPath(root)
    writeGeneratedTypes(schema, outFile)
    console.log(`wrote ${outFile}`)
    return
  }

  if (command === 'check') {
    const schema = await schemaFromConfig(configPath)
    const outFile = defaultGeneratedTypesPath(root)
    const result = checkGeneratedTypes(schema, outFile)
    if (!result.ok) {
      console.error(`atomic-css check failed: ${outFile} is missing or stale`)
      process.exit(1)
    }
    console.log(`ok ${outFile}`)
    return
  }

  console.log(`atomic-css — commands: generate | check
Usage:
  atomic-css generate [--config atomic.config.ts]
  atomic-css check [--config atomic.config.ts]`)
  process.exit(command === 'help' ? 0 : 1)
}

async function schemaFromConfig(path: string) {
  if (!existsSync(path)) {
    return buildAtomicSchema({ preset: 'preset-mini' })
  }
  const loaded = await loadAtomicConfig(path)
  return buildAtomicSchema(loaded.config)
}

main().catch((error: unknown) => {
  console.error(error instanceof Error ? error.message : error)
  process.exit(1)
})
