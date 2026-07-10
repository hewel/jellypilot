/** Astryx v0.1.4 CSS variable identifiers with Neutral defaults. */
export const neutralTokenValues = {
  colors: {
    background: 'var(--colors-background, #ffffff)',
    foreground: 'var(--colors-foreground, #0a0a0a)',
    muted: 'var(--colors-muted, #f4f4f5)',
    mutedForeground: 'var(--colors-muted-foreground, #71717a)',
    border: 'var(--colors-border, #e4e4e7)',
    primary: 'var(--colors-primary, #18181b)',
    primaryForeground: 'var(--colors-primary-foreground, #fafafa)',
  },
  space: {
    1: 'var(--space-1, 0.25rem)',
    2: 'var(--space-2, 0.5rem)',
    3: 'var(--space-3, 0.75rem)',
    4: 'var(--space-4, 1rem)',
    6: 'var(--space-6, 1.5rem)',
    8: 'var(--space-8, 2rem)',
  },
  radii: {
    sm: 'var(--radii-sm, 0.25rem)',
    md: 'var(--radii-md, 0.375rem)',
    lg: 'var(--radii-lg, 0.5rem)',
    full: 'var(--radii-full, 9999px)',
  },
  font: {
    sans: 'var(--font-sans, ui-sans-serif, system-ui, sans-serif)',
    mono: 'var(--font-mono, ui-monospace, SFMono-Regular, Menlo, monospace)',
  },
  fontSize: {
    sm: 'var(--font-size-sm, 0.875rem)',
    md: 'var(--font-size-md, 1rem)',
    lg: 'var(--font-size-lg, 1.125rem)',
    xl: 'var(--font-size-xl, 1.25rem)',
    '2xl': 'var(--font-size-2xl, 1.5rem)',
  },
  lineHeight: {
    tight: 'var(--line-height-tight, 1.25)',
    normal: 'var(--line-height-normal, 1.5)',
  },
} as const

export const neutralBreakpoints = {
  sm: '640px',
  md: '768px',
  lg: '1024px',
  xl: '1280px',
  '2xl': '1536px',
} as const
