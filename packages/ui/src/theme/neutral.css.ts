import { createGlobalTheme, globalStyle } from '@vanilla-extract/css'
import { neutralTokenValues } from './tokens'

export const neutralTokens = createGlobalTheme(':root', neutralTokenValues)
export { neutralBreakpoints } from './tokens'

const sharedPreset = {
  '--space-1': '0.25rem',
  '--space-2': '0.5rem',
  '--space-3': '0.75rem',
  '--space-4': '1rem',
  '--space-6': '1.5rem',
  '--space-8': '2rem',
  '--font-mono': 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
} as const

const neutralGeometry = {
  '--radii-sm': '0.25rem',
  '--radii-md': '0.375rem',
  '--radii-lg': '0.5rem',
  '--radii-full': '9999px',
} as const

const jellypilotGeometry = {
  '--radii-sm': '0.375rem',
  '--radii-md': '0.5rem',
  '--radii-lg': '0.75rem',
  '--radii-full': '9999px',
} as const

const presetSelector = (preset: 'neutral' | 'jellypilot', mode: 'light' | 'dark') =>
  [
    `:root[data-theme-id="${preset}"][data-theme="${mode}"]`,
    `[data-jp-theme][data-theme-id="${preset}"][data-theme="${mode}"]`,
    `[data-jp-layer-portal][data-theme-id="${preset}"][data-theme="${mode}"]`,
  ].join(', ')

globalStyle(presetSelector('neutral', 'light'), {
  vars: {
    ...sharedPreset,
    ...neutralGeometry,
    '--colors-background': '#ffffff',
    '--colors-foreground': '#18181b',
    '--colors-muted': '#f4f4f5',
    '--colors-muted-foreground': '#71717a',
    '--colors-border': '#e4e4e7',
    '--colors-primary': '#18181b',
    '--colors-primary-foreground': '#fafafa',
    '--font-sans': 'ui-sans-serif, system-ui, sans-serif',
  },
})

globalStyle(presetSelector('neutral', 'dark'), {
  vars: {
    ...sharedPreset,
    ...neutralGeometry,
    '--colors-background': '#09090b',
    '--colors-foreground': '#fafafa',
    '--colors-muted': '#27272a',
    '--colors-muted-foreground': '#a1a1aa',
    '--colors-border': '#3f3f46',
    '--colors-primary': '#fafafa',
    '--colors-primary-foreground': '#18181b',
    '--font-sans': 'ui-sans-serif, system-ui, sans-serif',
  },
})

globalStyle(presetSelector('jellypilot', 'light'), {
  vars: {
    ...sharedPreset,
    ...jellypilotGeometry,
    '--colors-background': '#f7f8fc',
    '--colors-foreground': '#191b23',
    '--colors-muted': '#ececf4',
    '--colors-muted-foreground': '#565866',
    '--colors-border': '#c6c5d0',
    '--colors-primary': '#4f46e5',
    '--colors-primary-foreground': '#ffffff',
    '--font-sans': "'Figtree Variable', ui-sans-serif, system-ui, sans-serif",
  },
})

globalStyle(presetSelector('jellypilot', 'dark'), {
  vars: {
    ...sharedPreset,
    ...jellypilotGeometry,
    '--colors-background': '#05060a',
    '--colors-foreground': '#f3f6ff',
    '--colors-muted': '#161b2a',
    '--colors-muted-foreground': '#aeb8cc',
    '--colors-border': '#262e42',
    '--colors-primary': '#818cf8',
    '--colors-primary-foreground': '#0b0a24',
    '--font-sans': "'Figtree Variable', ui-sans-serif, system-ui, sans-serif",
  },
})
