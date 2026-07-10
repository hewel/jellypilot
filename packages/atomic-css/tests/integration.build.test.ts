import { spawnSync } from 'node:child_process'
import { existsSync, readFileSync, readdirSync, rmSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { expect, test } from '@rstest/core'

const packageRoot = join(dirname(fileURLToPath(import.meta.url)), '..')
const fixturesRoot = join(packageRoot, 'tests/fixtures')

function buildFixture(name: 'client' | 'ssr'): {
  status: number
  stdout: string
  stderr: string
  distDir: string
} {
  const fixtureDir = join(fixturesRoot, name)
  const distDir = join(fixtureDir, 'dist')
  if (existsSync(distDir)) {
    rmSync(distDir, { recursive: true, force: true })
  }
  const result = spawnSync(
    'bunx',
    ['rsbuild', 'build', '-c', 'rsbuild.config.ts'],
    {
      cwd: fixtureDir,
      encoding: 'utf8',
      env: {
        ...process.env,
        NODE_PATH: join(packageRoot, 'node_modules'),
      },
    },
  )
  return {
    status: result.status ?? 1,
    stdout: result.stdout ?? '',
    stderr: result.stderr ?? '',
    distDir,
  }
}

function collectTextFiles(dir: string): string[] {
  if (!existsSync(dir)) return []
  const out: string[] = []
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name)
    if (entry.isDirectory()) {
      out.push(...collectTextFiles(path))
      continue
    }
    if (/\.(js|css|mjs|cjs)$/.test(entry.name)) {
      out.push(readFileSync(path, 'utf8'))
    }
  }
  return out
}

/** Node builds keep VE CSS in base64 virtual-loader path segments. */
function decodedAssetText(assets: string): string {
  const decoded: string[] = [assets]
  for (const match of assets.matchAll(/source_([A-Za-z0-9+/=]{16,})_/g)) {
    const payload = match[1]
    if (payload === undefined) continue
    try {
      decoded.push(Buffer.from(payload, 'base64').toString('utf8'))
    } catch {
      // ignore non-base64 segments
    }
  }
  return decoded.join('\n')
}

test('client Rsbuild compiles atomic() into ordinary CSS', () => {
  const result = buildFixture('client')
  expect(result.status, result.stderr || result.stdout).toBe(0)
  const assets = collectTextFiles(result.distDir).join('\n')
  expect(assets).toMatch(/display\s*:\s*flex/)
  expect(assets).not.toContain('must be compiled by pluginAtomic')
  expect(assets).not.toContain('@unocss/preset-mini')
  expect(assets).not.toContain('css-tree')
})

test('SSR Rsbuild compiles atomic() into ordinary CSS', () => {
  const result = buildFixture('ssr')
  expect(result.status, result.stderr || result.stdout).toBe(0)
  const assets = decodedAssetText(collectTextFiles(result.distDir).join('\n'))
  expect(assets).toMatch(/display\s*:\s*flex/)
  expect(assets).not.toContain('must be compiled by pluginAtomic')
})

test('Rstest compiles atomic() through pluginAtomic', () => {
  const fixtureDir = join(fixturesRoot, 'rstest')
  const result = spawnSync(
    'bunx',
    ['rstest', '-c', 'rstest.config.ts'],
    {
      cwd: fixtureDir,
      encoding: 'utf8',
      env: process.env,
    },
  )
  expect(result.status ?? 1, result.stderr || result.stdout).toBe(0)
  expect(result.stdout ?? '').toMatch(/pass|passed/i)
})

test('npm pack --dry-run only includes public package files', () => {
  const result = spawnSync('npm', ['pack', '--dry-run'], {
    cwd: packageRoot,
    encoding: 'utf8',
  })
  expect(result.status ?? 1, result.stderr || result.stdout).toBe(0)
  const output = `${result.stdout}\n${result.stderr}`
  expect(output).toContain('dist/index.js')
  expect(output).toContain('dist/rsbuild.js')
  expect(output).toContain('dist/preset-mini.js')
  expect(output).toContain('dist/cli.js')
  expect(output).not.toContain('tests/')
  expect(output).not.toContain('src/')
  expect(output).not.toContain('.atomic-css')
})
