export const PINNED_HOSTS = {
  '@rsbuild/core': '2.0.15',
  '@rspack/core': '2.0.8',
  '@vanilla-extract/css': '1.20.1',
  '@vanilla-extract/webpack-plugin': '2.3.27',
  '@unocss/preset-mini': '66.7.4',
} as const

export type HostPackage = keyof typeof PINNED_HOSTS

export function assertHostVersions(
  resolveVersion: (name: HostPackage) => string | undefined,
): void {
  const mismatches: string[] = []
  for (const name of Object.keys(PINNED_HOSTS) as HostPackage[]) {
    const expected = PINNED_HOSTS[name]
    const actual = resolveVersion(name)
    if (actual === undefined) {
      mismatches.push(`${name}: missing (expected ${expected})`)
      continue
    }
    if (actual !== expected) {
      mismatches.push(`${name}: resolved ${actual}, expected ${expected}`)
    }
  }
  if (mismatches.length > 0) {
    throw new Error(
      `atomic-css host version drift:\n${mismatches.map((line) => `  - ${line}`).join('\n')}`,
    )
  }
}
