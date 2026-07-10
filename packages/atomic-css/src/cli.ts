#!/usr/bin/env node
const [command = 'help'] = process.argv.slice(2)

if (command === 'generate') {
  console.log(
    'atomic-css generate: project type generation lands with the schema slice (#134).',
  )
  process.exit(0)
}

if (command === 'check') {
  console.log(
    'atomic-css check: stale-type checking lands with the schema slice (#134).',
  )
  process.exit(0)
}

console.log(`atomic-css — commands: generate | check
Usage: atomic-css <command>`)
process.exit(command === 'help' ? 0 : 1)
