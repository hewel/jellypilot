export type ThemeMode = 'light' | 'dark'
export type ThemePreference = 'system' | ThemeMode

export type ThemeDescriptor = {
  readonly id: string
  readonly fonts: {
    readonly sans: string
    readonly mono: string
  }
  readonly colors: {
    readonly background: string
    readonly foreground: string
  }
  readonly space: Readonly<Record<string, string>>
  readonly radii: Readonly<Record<string, string>>
}

export type ThemeContextValue = {
  readonly preference: ThemePreference
  readonly resolved: ThemeMode
  readonly descriptor: ThemeDescriptor
}
