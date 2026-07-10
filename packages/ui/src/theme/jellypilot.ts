import { defineTheme } from './defineTheme'

/** JellyPilot preset owns Figtree + independent geometry/color/motion values. */
export const jellypilotTheme = defineTheme({
  id: 'jellypilot',
  fonts: {
    sans: 'Figtree, ui-sans-serif, system-ui, sans-serif',
    mono: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
  },
  colors: {
    background: '#05060a',
    foreground: '#f3f6ff',
  },
  space: {
    1: '0.25rem',
    2: '0.5rem',
    3: '0.75rem',
    4: '1rem',
    6: '1.5rem',
    8: '2rem',
  },
  radii: {
    sm: '0.375rem',
    md: '0.5rem',
    lg: '0.75rem',
    full: '9999px',
  },
})
