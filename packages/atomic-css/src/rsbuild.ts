import { createRequire } from 'node:module'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import type { RsbuildPlugin } from '@rsbuild/core'
import type {
  EntryDescription,
  EntryItem,
  EntryObject,
} from '@rspack/core'
import { VanillaExtractPlugin } from '@vanilla-extract/webpack-plugin'
import { ensureGeneratedBaseline } from './compiler/generate.js'
import { assertHostVersions, type HostPackage } from './compiler/hosts.js'

const require = createRequire(import.meta.url)
const packageRoot = join(dirname(fileURLToPath(import.meta.url)), '..')

export type PluginAtomicOptions = {
  /** Directory for real-file generated modules (default: <cwd>/.atomic-css). */
  generatedDir?: string
}

export function pluginAtomic(options: PluginAtomicOptions = {}): RsbuildPlugin {
  return {
    name: 'plugin-atomic-css',
    setup(api) {
      const root = api.context.rootPath
      const generatedDir = options.generatedDir ?? join(root, '.atomic-css')
      const { registryPath } = ensureGeneratedBaseline({ outDir: generatedDir })

      assertHostVersions((name: HostPackage) => {
        try {
          const pkgPath = require.resolve(`${name}/package.json`, {
            paths: [root, packageRoot],
          })
          // require() returns any; version is validated as string below.
          const pkg: unknown = require(pkgPath)
          if (
            pkg &&
            typeof pkg === 'object' &&
            'version' in pkg &&
            typeof pkg.version === 'string'
          ) {
            return pkg.version
          }
          return undefined
        } catch {
          return undefined
        }
      })

      api.modifyRspackConfig((config) => {
        const plugins = config.plugins ?? []
        const withoutVe = plugins.filter((plugin) => {
          if (!plugin || typeof plugin !== 'object') return true
          if (!('constructor' in plugin)) return true
          return plugin.constructor?.name !== 'VanillaExtractPlugin'
        })
        config.plugins = [...withoutVe, new VanillaExtractPlugin()]

        const entry = config.entry
        if (isEntryObject(entry)) {
          for (const key of Object.keys(entry)) {
            entry[key] = prependRegistry(entry[key]!, registryPath)
          }
        }

        const rewriteLoader = join(
          dirname(fileURLToPath(import.meta.url)),
          'loaders',
          'rewrite-loader.js',
        )
        const rules = config.module?.rules ?? []
        rules.unshift({
          test: /\.css\.ts$/,
          enforce: 'pre',
          use: [{ loader: rewriteLoader }],
        })
        config.module = config.module ?? {}
        config.module.rules = rules
        return config
      })
    },
  }
}

function isEntryObject(entry: unknown): entry is EntryObject {
  return (
    typeof entry === 'object' &&
    entry !== null &&
    !Array.isArray(entry) &&
    typeof entry !== 'function'
  )
}

function prependRegistry(
  entryValue: EntryItem | EntryDescription,
  registryPath: string,
): EntryItem | EntryDescription {
  if (typeof entryValue === 'string') {
    return [registryPath, entryValue]
  }
  if (Array.isArray(entryValue)) {
    if (entryValue.includes(registryPath)) return entryValue
    return [registryPath, ...entryValue]
  }
  if (isEntryDescription(entryValue)) {
    const current = entryValue.import
    if (typeof current === 'string') {
      return { ...entryValue, import: [registryPath, current] }
    }
    if (Array.isArray(current)) {
      if (current.includes(registryPath)) return entryValue
      return { ...entryValue, import: [registryPath, ...current] }
    }
  }
  return entryValue
}

function isEntryDescription(
  value: EntryItem | EntryDescription,
): value is EntryDescription {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}
