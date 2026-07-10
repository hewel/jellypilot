export type ThemeTokenRef = {
  readonly kind: 'theme-ref'
  readonly exportName: string
  readonly moduleId: string
}

export type ProjectThemeTokens = Record<string, string | ThemeTokenRef>

export type AtomicConfig = {
  preset: 'preset-mini'
  projectTheme?: {
    tokens?: ProjectThemeTokens
    conditions?: {
      dark?: string
      disabled?: string
    }
  }
  overrides?: Array<{
    family: string
    name: string
    value: string | ThemeTokenRef
  }>
}

export type LoadedConfig = {
  config: AtomicConfig
  sourcePath: string
}
